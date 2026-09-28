---
description: Rust 代码禁止 unsafe（块 / fn / impl / extern），只允许已批准的平台 FFI 兜底
condition:
  - '\bunsafe\s*\{'
  - '\bunsafe\s+(fn|impl|trait|extern)\b'
astCondition:
  - 'unsafe { $$$BODY }'
globs:
  - native/**/*.rs
  - crates/**/*.rs
repeatMode: after-gap
repeatGap: 2
---

你正在往 FluxDown 的 Rust 代码写入 `unsafe`。项目红线：**全部代码默认安全**，`unsafe` 只在「没有安全替代」的平台 FFI 场景被批准，且必须满足下面全部条件。

## 先自问：真的没有安全写法吗

`unsafe` 在本项目里几乎总是错误答案。先按顺序排除：

1. **std 已有安全 API**：`File::set_len` / `sync_all`、`std::fs::metadata`、`OsString` / `CString` / `Path` 转换、`std::process::Command` + `CommandExt::creation_flags`（Windows 隐藏黑窗）——都不需要 `unsafe`。
2. **已是直接依赖的安全封装**（无需新增依赖）：`directories`（home / 数据目录，engine / cli / agent / `crates/app` 已用）、`fs2`（`FileExt::allocate` 预分配、`available_space` 磁盘余量、文件锁；engine / daemon / agent / server 已用）、`tokio::fs`。其他 crate（`nix` / `rustix` / `sysinfo` / `core-foundation`）只是传递依赖，**不许**为了绕过 `unsafe` 直接引入——新增依赖需用户确认。
3. **改设计**：能靠 `Result` / `Option` / 生命周期表达的，就不该靠裸指针；「性能」不是理由，先测量。
4. **类型转换**：`bytemuck` / `zerocopy` 不是直接依赖，`transmute` / `from_raw_parts` / `slice::from_raw_parts` 一律禁止；改用 `to_le_bytes` / `from_le_bytes` / `chunks_exact` 等安全 API。

以上任一路走得通 → 删掉 `unsafe`，重写。

## 唯一被批准的例外：无安全封装的平台 FFI

只有「调用操作系统 C API，且 std 与现有依赖都没有安全封装」才允许，且**必须同时满足**：

- `#[cfg(target_os = …)]` / `#[cfg(unix)]` / `#[cfg(windows)]` 门控，非该平台零 `unsafe`。
- **每个 `unsafe` 块紧上方一行 `// SAFETY:` 注释**，写清被满足的前置条件（指针有效期、NUL 结尾、所有权归属、谁负责释放）。没有 SAFETY 注释 = 违规。
- `unsafe` 块**只包住那一条 FFI 调用**，参数准备、返回值检查、错误转换全放在块外；禁止把整段逻辑塞进一个大 `unsafe {}`。
- 对外暴露的是**安全函数**（签名无 `unsafe`），内部把 FFI 的失败映射为 `Result` / `Option`；禁止新增 `pub unsafe fn`。
- 资源型句柄（`CFStringRef`、`HGLOBAL`、`HANDLE`、`LocalAlloc` 内存）必须用 RAII 守卫（参考 `native/hub/src/macos_cf.rs::CfOwned`、`native/hub/src/clipboard_file.rs` 的剪贴板 guard），不许裸 `Release/Free` 散落在各分支。
- `unsafe extern "C" {}` / `unsafe extern "system" {}` 只声明当前文件确实调用的符号；能用 `windows_sys` 已导出的绑定就不要手写 `extern`。
- 禁止 `unsafe impl Send/Sync`：类型跨线程不安全就换结构（`Arc<Mutex<_>>`、消息通道），不要用 `unsafe impl` 把问题盖住。

**已批准的存量 FFI 站点**（改这些文件时保持现有形态，不扩大范围）：

|位置|用途|
|---|---|
|`native/engine/src/segment_coordinator.rs`|`libc::fallocate` / `posix_fadvise` / Win32 `SetFileInformationByHandle` 预分配|
|`native/engine/src/disk_space.rs`|`libc::statvfs` / `statfs` / `GetDiskFreeSpaceExW` 磁盘余量|
|`native/engine/src/bt_sparse.rs`、`bt_downloader.rs`|Win32 `DeviceIoControl(FSCTL_SET_SPARSE)` / `GetFileAttributesW`|
|`native/hub/src/{macos_cf,file_association,protocol_registry}.rs`、`native/agent/src/platform/*`|macOS CoreFoundation / LaunchServices、Win32 `SHChangeNotify`|
|`native/hub/src/{clipboard_file,reveal_file,shortcut_icon,native_messaging,nmh_registry}.rs`|Win32 剪贴板 / ShellExecute / 图标 / 命名管道 ACL、`getpwuid_r`|
|`native/fluxdown_updater/src/main.rs`、`native/nmh/src/main.rs`|进程等待 / 枚举（`OpenProcess` / Toolhelp / `libc::kill`）|

**engine crate 之外的下载逻辑、`native/api`、`native/protocol`、`native/daemon`、`native/server`、`crates/*`（GPUI）一律零 `unsafe`**——这些 crate 目前没有任何 `unsafe`，不要成为第一个。

## 修正方式

- 有安全替代 → 直接用安全写法重写，删掉 `unsafe`。
- 确属平台 FFI 例外 → 按上面清单补齐：`cfg` 门控 + `// SAFETY:` + 最小 `unsafe` 范围 + 安全外层函数 + RAII 守卫，并在回复里向用户**明确说明为何没有安全替代**。
- 测试代码（`#[cfg(test)]` / `tests/`）同样适用；只有验证平台行为的 Windows 专属测试（如 `native/engine/tests/bt_sparse_add.rs`）可沿用既有形态。
