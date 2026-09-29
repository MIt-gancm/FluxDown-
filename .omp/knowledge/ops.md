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

---

## 发布与 CI（`.github/workflows/release.yml`）

**组件变更检测**流水线，`v*` tag 触发。`changes` job diff `PREV..TAG` 映射路径→输出（`app`/`extension`/`server`/`mobile`/`cli`），首个 tag 全量构建。**分支守卫**：稳定 `vX.Y.Z` 必须是 `origin/stable` 祖先；预览 `vX.Y.Z-rc.N` 必须在 `origin/main`；否则整条失败。

**单组件补发**：`gh workflow run release.yml --ref main -f tag=<已有 v* tag> -f component=server`（也支持 `mobile`）。构建源码固定在输入 tag；Server Docker 仅从 workflow 提交覆盖 `.dockerignore` 与 `docker/server.Dockerfile`，以便修复打包而不移动已发布标签。只上传对应组件 Release，不重跑其余组件。Server 的版本号、预发布标记与 release notes 都取输入 tag；预览镜像不更新 `latest`。

路径→组件映射（要点）：`fluxDown/*`→extension；`web|native/agent|native/daemon|docker/*`→server；`assets/i18n/*`→app+mobile+server；`website-v2/src/lib/gpui-theme/*`→server（Web SPA 构建期别名引用）；`packaging/*`→app+server；`native/server/*`（已冻结）→不构建；`native/cli/*`→cli；`native/api|native/protocol/*`→app+server+cli；`native/engine/*`→app+server+mobile+cli；`android|lib/src/mobile/*`→mobile；`lib/*`→app+mobile；`website/*`/`docs/*`/`*.md`→不构建。

构建矩阵：Windows（x64+arm64，Inno 安装器+便携 zip）、扩展（Chrome+Firefox，预发布 tag 不打包扩展）、Linux（AppImage/deb/arch/tar.gz）、macOS（x64+arm64，DMG+便携）、Android（split-per-abi + universal APK，cargokit 编各 ABI cdylib）、Web SPA（一次复用）、server 多平台二进制（`fluxdown-agent`(web-ui) + `fluxdownd` 同包，入口 `fluxdown-agent --server`；musl 静态）、server NAS 包（OpenWrt/QNAP/群晖）、CLI 六平台、server Docker（ghcr.io `fluxdown-server`，tini 作 PID 1，QEMU arm64）。每个 release job 各用自己的组件 tag，跑 git-cliff（`--include-path <组件目录>`）后经 Claude Code CLI 翻译为中英双语（`<!-- fluxdown:lang:zh/en -->` 标记，失败回退原始 cliff）。

**下载分发**：每个 release job 在 GitHub Release 创建后经 `.github/actions/oss-upload`（固定版 ossutil 2.x）把 `release-assets/*` 同步到阿里云 OSS `oss://zerx-lab/FluxDownRelease/<版本>/<组件>/<文件>`（如 `v0.4.8/app/`、`v0.4.8/server/`；tag→路径规则在 `website/src/lib/oss.ts::releaseObjectKey` 与 action bash 各一份，须同步。bucket 私有；secrets `OSS_ACCESS_KEY_ID`/`OSS_ACCESS_KEY_SECRET`，未配则跳过，`continue-on-error` 不阻断发布）。官网 `website/src/pages/api/download/[filename].ts` 优先做预签名 HEAD 探测后 302 到 1h 预签名 GET（V1 签名），OSS 缺失/不可达回退 GitHub CDN；`?source=github` 强制直连。不再有 CN 地域分流与 githubProxy 镜像。

构建期 dart-define：`APP_VERSION`、`ANALYTICS_APP_KEY`、`FLUXCLOUD_BASE_URL`、`STATS_*`。

**macOS 签名/公证 secrets（2026-09-28 全部配置到 `zerx-lab/FluxDown`，workflow 尚未接入）**：分发走 DMG（Developer ID + notarytool），不上 Mac App Store。Team ID `KD4N89AAF5`（个人账户 Yunhua Qu）。`MACOS_CERT_P12_BASE64`（Developer ID Application 证书 + 私钥的 .p12，base64；证书 SHA1 `A14C6036…DAB3C`，2031-09-13 到期）、`MACOS_CERT_PASSWORD`（.p12 导出密码）、`APPLE_API_KEY_P8_BASE64`（App Store Connect 团队 API 密钥 .p8，base64，角色「开发者」）、`APPLE_API_KEY_ID`、`APPLE_API_ISSUER_ID`。公证用 `xcrun notarytool submit --key <p8> --key-id … --issuer …`，不用 Apple ID + App 专用密码。值只在 GitHub Secrets，**禁止**把 .p12/.p8/密码提交进仓库。

`fluxdown-agent` 的 FluxCloud 地址解析（`native/agent/src/runtime.rs`）：运行期 env `FLUXCLOUD_BASE_URL` > 编译期同名 env（`option_env!`，正式包应在 `cargo build` 时注入，与 dart-define 同源）> `http://127.0.0.1:8720`。**仅调试构建**再叠加 agent 私有状态里的用户覆盖（`agent.cloud.endpointGet/Set`，GPUI 账户页「服务器地址」卡片；对齐 Flutter `CloudApiConfig` 的 `kDebugMode` 门控），正式构建忽略残留覆盖并拒绝 `endpointSet`。

**GPUI 调试包**（`.github/workflows/gpui-debug-package.yml`，仅 `workflow_dispatch`，不发 Release）：输入 `ref` / `platform`（windows·linux·macos·all）/ `arch`（x64·arm64·all）/ `build_mode`，产出 `fluxdown-desktop` + `fluxdown-agent` + `fluxdownd` 同目录压缩包（Windows zip 带 MSVC CRT 与 `.pdb`，Unix tar.gz 保留可执行位）。始终走 `--release`：`gpui_windows` 在 `debug_assertions` 下运行期按构建机绝对路径读 `shaders.hlsl`，debug 包换机即失效；`fast` 模式只经 `CARGO_PROFILE_RELEASE_*` 关 LTO、保留行号符号。同理 macOS 主机无法交叉出 Windows release 包（着色器 `fxc` 预编译只在 Windows 主机的 build.rs 执行）。

---

## 设计文档实现状态（`docs/`）

> ⚠️ `docs/` 在 `.gitignore` 里（零文件入库），下列设计文档**只存在于本机工作副本**；契约与不变式一律写回 `AGENTS.md` / 本目录，别只留在 `docs/`。

避免混淆——**已实现** vs **仅设计**：
- **已实现**：多文件任务组（`multi-file-task-group-design.md`）、插件系统 + 去中心化市场（`fluxdown-plugin-marketplace-plan.md` 等）。
- **部分实现（客户端 + agent）**：多设备协作 / FluxCloud 配置同步（`multi-device-collab-design.md`）——Flutter `lib/src/services/cloud/` 与 `native/agent`（云同步/远程任务，GPUI 与 headless Web SPA 经 `/rpc` 使用）已落地，对接**外部 L2 relay**；打洞/E2E 仍设计阶段；LAN 设备互联（配对/mDNS）在新架构未实现。
- **仅设计（无引擎/服务器代码）**：浏览器扩展嗅探规则市场（`sniff-rule-market-design.md`——云端锚定 FluxCloud，扩展侧 `sniff-engine.ts` + FluxCloud `sniff_packs` 表均未落地；文档含三轮对抗评审记录与逐条打折清单）。
- **已实现（全端）**：RSS 订阅自动下载（`rss-subscription-design.md`，issue #97）——引擎 `native/engine/src/rss/`、REST `/api/v1/rss/*`、WS `rssSourcesChanged`/`rssItemsChanged`、hub 信号、桌面 UI（侧边栏区块 + 条目流 + 三 Tab 对话框 + 两步向导）、web SPA 同构、CLI `fluxdown rss`、MCP `rss_list`/`rss_add`/`rss_remove`。
- **已实现（免费层，全宿主）**：webhook 任务事件通知（`webhook-notification-design.md`）——引擎 `native/engine/src/webhook.rs`（6 事件 × 8 预设 + 占位符模板 + HMAC 签名 + 环形投递日志）、daemon RPC `daemon.webhook.{get,test,simulate,clearDeliveries}`、hub 信号、GPUI/Web「Webhook」页。端点表就是 config 键 `webhook.endpoints`，桌面 / headless / CLI `--local` 共享。**付费托管 Relay（设计 §6）未实现**，客户端无任何 relay 代码。
- **命名歧义警告**：引擎里的 `tracker_subscription.rs` / `ed2k/server_subscription.rs` 指 **BT tracker 列表 / ED2K server.met 订阅**，与 `rss/` 的 feed 订阅是两回事；官网 `api/webhooks/github` 是 GitHub 接收器，与 `engine/src/webhook.rs` 的任务事件推送无关。
