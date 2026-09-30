# FluxDown internals · 日志 · 发布与 CI · 设计文档实现状态

> 本文件是 `FluxDown/AGENTS.md` 的深挖附录：只放**枚举性 / 可从源码复原**的细节，硬不变式与红线在 AGENTS.md。
> 路径以 `FluxDown/` 为根（cwd=工作区根时前置 `FluxDown/`）。事实层以源码为准，文档给坐标。

---

## 日志系统

Dart 与 Rust 两端写**同一目录同一文件**，统一格式 `HH:MM:SS.mmm [Tag] message`。
- 目录：Windows exe 同级 `logs/`；Linux `~/.local/share/fluxdown/logs/`。文件名 `fluxdown_YYYY-MM-DD.log`，单卷 2MB 分卷，总量超 `log_max_size_mb`（默认 10MB）从最旧删，保留 7 天。
- Dart：`import '../services/log_service.dart'; logInfo(_tag, msg); logError(...)`。
- Rust：`use crate::logger::log_info; log_info!("[mod] ...")`（Rust 2024 无 `#[macro_use]`，每文件显式 use）。
- 导出：设置「关于」→ ZIP（纯 Dart 标准库，零依赖）。
- GPUI 任务「日志」另走引擎数据库中的结构化活动历史（`engine/task_activity.rs`、`db.rs`），不是全局滚动文本文件：源端时间、跨重启 ID、查询分页与实时通知配合。当前保留七天且全库最多五万条，两者先到先清理；页面显示保留截断。同步事件入有界队列，持久化失败不广播成功记录，队列溢出/失败形成显式 `journal_overflow` 缺口；关机冲刷有界，不能因日志数据库不可用永久挂住。

**GPUI 桌面链路（desktop / agent / daemon）**——与上面的 Flutter/引擎滚动日志分开：
- `fluxdownd`：沿用引擎 `logger`（`<engine 数据目录>/logs/fluxdown_*.log`）；`main` 返回错误时带完整错误链落盘。
- `fluxdown-desktop` 与 `fluxdown-agent`（非 `--server`）：共用 `native/logfile`（`fluxdown_logfile`，零依赖），写 `<agent 数据目录>/logs/{desktop,agent}.log`。单文件 2MB 轮转为 `.log.1`（只留一份，每进程 ≤4MB）；每个文件开头有会话头（版本、exe、参数形态、平台、`SESSIONNAME`/显示协议、`GPUI_*`/`ZED_*` 原值、其余 `FLUXDOWN_*` 只记名字），轮转后重写；同一消息（数字归一）每 60s 只落 10 条，其余汇总成 `[suppressed N repeats over Ts]`；UTC 时间戳；panic 带回溯。
- desktop 收 `log` 门面：`fluxdown*` 与 `gpui*`（含显卡选择、D3D 特性级别、DirectComposition、设备丢失、draw 失败）按 `FLUXDOWN_LOG_LEVEL`（缺省 info），第三方只收 warn+；另记窗口打开参数、首个窗口 GPU 规格（软件渲染告警）、首帧耗时 / 10s 无首帧告警、UI 线程心跳看门狗（停摆 10/60/300s 告警，macOS 因 App Nap 不启用）、agent 连接状态迁移与 agent 进程拉起/退出。启动参数只记开关与位置参数个数，不记链接。
- agent 收 `tracing`：`RUST_LOG` 非空时原样生效，否则 `warn` + 第一方 crate 按 `FLUXDOWN_LOG_LEVEL`；daemon 子进程 stderr 追加到同目录 `fluxdownd.stderr.log`（超 1MB 截断），拉起 / 非零退出（含存活时长）记入 `agent.log`。`--server` 模式仍是 stderr tracing。
- Doctor 的日志目录检查、「打开日志目录」、日志导出都指向 `<agent 数据目录>/logs`（`log_export::agent_log_dir`）。

---

## 发布与 CI（`.github/workflows/release.yml`）

**组件变更检测 + 统一 release** 流水线，`v*` tag 触发。`changes` job diff `PREV..TAG` 映射路径→输出（`app`/`extension`/`server`/`mobile`/`cli`），首个 tag 全量构建；**基线 `PREV`**：稳定 `vX.Y.Z` 取上一个稳定版（`git describe --exclude 'v*-*'`，跳过其间全部 `-rc.N`，rc 阶段改过的组件都进稳定版），预览取上一个任意 `v*` tag；同一基线经 `changes.outputs.prev` 传给 release notes（稳定版 git-cliff 以 `GIT_CLIFF__GIT__IGNORE_TAGS` 忽略 rc tag，说明覆盖整个 rc 周期）。同时解析构建源码 `source`（默认 = tag 提交）。**分支守卫**：稳定 `vX.Y.Z` 必须是 `origin/stable` 祖先；预览 `vX.Y.Z-rc.N` 必须在 `origin/main`；否则整条失败。同一 tag 的运行由 `concurrency` 串行。

**浏览器扩展**与其他组件同一套变更判定：预览 tag 有变动只打包附到 release（Chrome zip + `FluxDown-<版本>-firefox-unsigned.zip`，manifest `version` 由 WXT 去掉预发布后缀、Chrome 带 `version_name`），**不推 Chrome/Edge 商店、不走 AMO 签名**（unlisted 签名也占 AMO 版本号，会与稳定版 `X.Y.Z.1` 冲突）；稳定 tag 有变动才打包 + 推三家商店 + AMO 签名 XPI。官网 `/api/release` 的扩展恒取稳定版。

**一个 tag 一个 release**：`prepare-release` 生成双语说明并建草稿 `vX.Y.Z`（预览标 prerelease）→ 各 `build-*` 并行 → `upload-<组件>` 经 `.github/actions/release-upload` 上传产物（`--clobber`），**最后**上传完成哨兵 `SHA256SUMS-<组件>.txt`（补发时先删旧哨兵、清掉旧清单里不再产出的资产）→ `publish-release`（`.github/scripts/release_publish.py`，`always()`）合并哨兵为 `SHA256SUMS.txt`、刷新说明头部（`<!-- fluxdown:release:begin … end -->`：已发布组件清单 + 服务器/CLI 安装说明，位于双语标记之前）、草稿转正；**latest 仅给「稳定版 + 桌面端完整 + 最高稳定版本」**。组件互不阻断：某组件失败只跳过它的 upload，其余照常发布，publish 以失败结束并在 job summary 列出补发命令；Server Docker 直推 ghcr、不阻断二进制上传，失败单独报告。官网 `/api/release` 只认有哨兵的组件（`website*/src/lib/release-assets.ts`，两站逐字一致），缺失组件回落到它上一个完整版本；`/api/changelog` 剥掉说明头部。历史拆分时代的 `server-v*`/`cli-v*`/`mobile-v*`/`extension-v*` release 仍被官网兼容识别，不再新建。

**补救**：偶发失败（源码无需改）→ 运行页 “Re-run failed jobs”（只重跑失败组件 + upload/publish；artifact 被后续 tag 的清理删掉后改用补发）。需改代码/打包 → 修复合入发布分支后 `gh workflow run release.yml --ref main -f tag=<已有 v* tag> -f component=<app|extension|server|cli|mobile|changed> [-f source_ref=<分支|提交>]`：`source_ref` 须包含该 tag 且在发布分支上，缺省 = tag 本身；版本号、预发布标记恒取 tag；`changed` 按该 tag 的变更检测补发全部应发组件；源码 ≠ tag 时补跑引擎门禁。源码取 tag 本身时 Server Docker 仅从 workflow 提交覆盖 `.dockerignore` 与 `docker/server.Dockerfile`（修打包不动源码）。拆分时代的旧 release（无哨兵）拒绝补发。预览镜像不更新 `latest`。

路径→组件映射（要点）：`fluxDown/*`→extension；`web|docker/*`→server；`native/agent|native/daemon/*`→app+server；`crates/*`→app（GPUI 桌面）；`assets/i18n/*`→app+mobile+server；`website-v2/src/lib/gpui-theme/*`→server（Web SPA 构建期别名引用）；`packaging/*`→app+server；`native/server/*`（已冻结）→不构建；`native/cli/*`→cli；`native/api|native/protocol/*`→app+server+cli；`native/engine/*`→app+server+mobile+cli；`android|lib/*|pubspec.*|native/hub/*`→mobile（Flutter 只剩 Android）；`native/fluxdown_updater/*`→不构建；`website/*`/`docs/*`/`*.md`→不构建。

构建矩阵：**桌面端 = GPUI**（`fluxdown-desktop` + `fluxdown-agent` + `fluxdownd` + `fluxdown_nmh` 四个可执行同目录，`--features fluxdown_agent/desktop`，必须 release profile），产物名与 Flutter 时代一致，旧 Flutter 客户端自动更新直接升级为 GPUI：Windows（x64+arm64，`installer/windows/setup.iss` 沿用 AppId 覆盖升级、`[InstallDelete]` 清掉 Flutter 残留、自启指向 `fluxdown-agent.exe --autostart`；便携 zip 带 `portable` 标记与 `flux_down.exe`=desktop 副本供 Flutter 更新器重启；**Windows 全部产物暂不签名**，SignPath 开源审核未过）、Linux x64（ubuntu-22.04，`scripts/package_gpui_linux.sh` 出 tar.gz/AppImage/deb/Arch；deb/Arch 装 `/opt/fluxdown` + `/usr/bin` 符号链接；AppImage 的 AppRun 把 `--autostart` 交给 agent，并等常驻 agent/daemon 退出后才返回以保持挂载；agent 在 AppImage 内把自启写成 `$APPIMAGE --autostart`；CI 装 deb 冒烟 + `ldd`）、macOS（x64 `macos-15-intel` / arm64 `macos-15` 分别构建，`scripts/package_gpui_macos.sh`：`FluxDown.app` + `Helpers/FluxDownAgent.app`（agent/daemon/nmh）→ Developer ID + Hardened Runtime 签名 → DMG → notarytool 公证 → DMG 与 .app 均 staple → tar.gz；缺签名 secrets 直接失败）；扩展（Chrome+Firefox，预发布 tag 不打包扩展）、Android（split-per-abi + universal APK，cargokit 编各 ABI cdylib）、Web SPA（一次复用）、server 多平台二进制（`fluxdown-agent`(web-ui) + `fluxdownd` 同包，入口 `fluxdown-agent --server`；musl 静态）、server NAS 包（OpenWrt/QNAP/群晖）、CLI 六平台、server Docker（ghcr.io `fluxdown-server`，tini 作 PID 1，QEMU arm64）。说明只在 tag 推送时生成一次：git-cliff（全仓）后经 Claude Code CLI 改写为中英双语（`<!-- fluxdown:lang:zh/en -->` 标记，失败回退原始 cliff）；补发不改已有说明。

**Windows 安装模式**：`setup.iss` 只做每用户安装（`PrivilegesRequired=lowest`，**不开** `PrivilegesRequiredOverridesAllowed`——开了对话框后一旦选「为所有用户」，Inno 的 `UsePreviousPrivileges` 会让之后每次安装 / 静默更新都要 UAC），`{autopf}` = `%LOCALAPPDATA%\Programs\FluxDown`；运行期集成（NMH、URL 协议、`.torrent`、自启、AUMID）全在 HKCU / `%LOCALAPPDATA%\FluxDown`。检测到 HKLM 下的旧全体用户安装时，`RemoveLegacyAllUsersInstall` 提权一次跑其卸载器，并把卸载器删掉的 HKCU 自启 / `.torrent` / URL 协议改指新目录；NMH 由新 agent 启动时 `auto_register` 重注册。

**下载分发**：每个 upload job 在上传 GitHub Release 后经 `.github/actions/oss-upload`（固定版 ossutil 2.x）把 `release-assets/*` 同步到阿里云 OSS `oss://zerx-lab/FluxDownRelease/<版本>/<组件>/<文件>`；目录由 release tag 推导，统一 release 的全部组件都落在 `vX.Y.Z/app/`（历史目录名），历史组件 release 在 `v0.4.8/server/` 等（tag→路径规则在 `website*/src/lib/oss.ts::releaseObjectKey` 与 action bash 各一份，须同步。bucket 私有；secrets `OSS_ACCESS_KEY_ID`/`OSS_ACCESS_KEY_SECRET`，未配则跳过，`continue-on-error` 不阻断发布）。官网 `website/src/pages/api/download/[filename].ts` 优先做预签名 HEAD 探测后 302 到 1h 预签名 GET（V1 签名），OSS 缺失/不可达回退 GitHub CDN；`?source=github` 强制直连。不再有 CN 地域分流与 githubProxy 镜像。

构建期注入：GPUI/agent 用 env `FLUXCLOUD_BASE_URL`（空则不导出）与 `FLUXDOWN_ANALYTICS_APP_KEY`；Android（Flutter）用 dart-define `APP_VERSION`、`FLUXCLOUD_BASE_URL`。

**macOS 签名/公证 secrets（已接入 release.yml 的 `build-macos`）**：分发走 DMG（Developer ID + notarytool），不上 Mac App Store。Team ID `KD4N89AAF5`（个人账户 Yunhua Qu）。`MACOS_CERT_P12_BASE64`（Developer ID Application 证书 + 私钥的 .p12，base64；证书 SHA1 `A14C6036…DAB3C`，2031-09-13 到期）、`MACOS_CERT_PASSWORD`（.p12 导出密码）、`APPLE_API_KEY_P8_BASE64`（App Store Connect 团队 API 密钥 .p8，base64，角色「开发者」）、`APPLE_API_KEY_ID`、`APPLE_API_ISSUER_ID`。公证用 `xcrun notarytool submit --key <p8> --key-id … --issuer …`，不用 Apple ID + App 专用密码。值只在 GitHub Secrets，**禁止**把 .p12/.p8/密码提交进仓库。

`fluxdown-agent` 的 FluxCloud 地址解析（`native/agent/src/runtime.rs`）：运行期 env `FLUXCLOUD_BASE_URL` > 编译期同名 env（`option_env!`，正式包应在 `cargo build` 时注入，与 dart-define 同源）> `http://127.0.0.1:8720`。**仅调试构建**再叠加 agent 私有状态里的用户覆盖（`agent.cloud.endpointGet/Set`，GPUI 账户页「服务器地址」卡片；对齐 Flutter `CloudApiConfig` 的 `kDebugMode` 门控），正式构建忽略残留覆盖并拒绝 `endpointSet`。

**GPUI 调试包**（`.github/workflows/gpui-debug-package.yml`，仅 `workflow_dispatch`，不发 Release）：输入 `ref` / `platform`（windows·linux·macos·all）/ `arch`（x64·arm64·all）/ `build_mode`，产出 `fluxdown-desktop` + `fluxdown-agent` + `fluxdownd` 同目录压缩包（Windows zip 带 MSVC CRT 与 `.pdb`，Unix tar.gz 保留可执行位）。始终走 `--release`：`gpui_windows` 在 `debug_assertions` 下运行期按构建机绝对路径读 `shaders.hlsl`，debug 包换机即失效；`fast` 模式只经 `CARGO_PROFILE_RELEASE_*` 关 LTO、保留行号符号。同理 macOS 主机无法交叉出 Windows release 包（着色器 `fxc` 预编译只在 Windows 主机的 build.rs 执行）。

---

## 设计文档实现状态（`docs/`）

> ⚠️ `docs/` 在 `.gitignore` 里（零文件入库），下列设计文档**只存在于本机工作副本**；契约与不变式一律写回 `AGENTS.md` / 本目录，别只留在 `docs/`。

避免混淆——**已实现** vs **仅设计**：
- **已实现**：多文件任务组（`multi-file-task-group-design.md`）、插件系统 + 去中心化市场（`fluxdown-plugin-marketplace-plan.md` 等）。
- **部分实现（客户端 + agent）**：多设备协作 / FluxCloud 配置同步（`multi-device-collab-design.md`）——Flutter `lib/src/services/cloud/` 与 `native/agent`（云同步 / 远程任务 / 设备元数据，GPUI 与 headless Web SPA 经 `/rpc` 使用）已落地；LAN 设备互联 L1（配对 / mDNS / 直连下发）已在 agent 实现（协议 crate `native/link`）；L2 跨网发现 / 打洞 / 中继与 E2E 仍在设计阶段（FluxCloud 无对应端点）。
- **仅设计（无引擎/服务器代码）**：浏览器扩展嗅探规则市场（`sniff-rule-market-design.md`——云端锚定 FluxCloud，扩展侧 `sniff-engine.ts` + FluxCloud `sniff_packs` 表均未落地；文档含三轮对抗评审记录与逐条打折清单）。
- **已实现（全端）**：RSS 订阅自动下载（`rss-subscription-design.md`，issue #97）——引擎 `native/engine/src/rss/`、REST `/api/v1/rss/*`、WS `rssSourcesChanged`/`rssItemsChanged`、hub 信号、桌面 UI（侧边栏区块 + 条目流 + 三 Tab 对话框 + 两步向导）、web SPA 同构、CLI `fluxdown rss`、MCP `rss_list`/`rss_add`/`rss_remove`。
- **已实现（免费层，全宿主）**：webhook 任务事件通知（`webhook-notification-design.md`）——引擎 `native/engine/src/webhook.rs`（6 事件 × 8 预设 + 占位符模板 + HMAC 签名 + 环形投递日志）、daemon RPC `daemon.webhook.{get,test,simulate,clearDeliveries}`、hub 信号、GPUI/Web「Webhook」页。端点表就是 config 键 `webhook.endpoints`，桌面 / headless / CLI `--local` 共享。**付费托管 Relay（设计 §6）未实现**，客户端无任何 relay 代码。
- **命名歧义警告**：引擎里的 `tracker_subscription.rs` / `ed2k/server_subscription.rs` 指 **BT tracker 列表 / ED2K server.met 订阅**，与 `rss/` 的 feed 订阅是两回事；官网 `api/webhooks/github` 是 GitHub 接收器，与 `engine/src/webhook.rs` 的任务事件推送无关。
