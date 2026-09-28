# FluxDown 粘性红线

仅收录 AGENTS.md 未覆盖的硬约束（其余规范见项目根 AGENTS.md，已自动加载）：

- UI 目标消歧：未来 PC 客户端将从 Flutter 迁移到 GPUI。用户明确说“GPUI 项目”或“GPUI UI”才修改 `crates/app`（包 `fluxdown_ui_app`）及 `crates/{i18n,theme,components,shell}`；未明确指定 GPUI 的“FluxDown UI/PC 客户端/UI 层”一律仍指当前默认 Flutter `lib/`。
- 禁止未经用户明确要求执行 git commit / push / tag；推送 v* tag 会直接触发 GitHub Actions 全平台发布流水线，属不可逆操作。
- 分支模型：`main` = 开发分支（超集 / 最新），`stable` = 稳定分支（子集）。日常开发一律在 `main`；禁止直接向 `stable` 提交功能。
- `stable` 只能通过合并 `main`（或从 `main` cherry-pick）前进；hotfix 若直接进 `stable`，必须同回合同步回 `main`。
- 一致性判定：`git log stable --not main --oneline` 必须为空。任何操作后不为空即违规，先修复再继续。
- 稳定 tag `vX.Y.Z` 只从 `stable` 打；预览 tag `vX.Y.Z-rc.N` 只从 `main` 打。
- GPUI 下载页侧栏：「状态」文件夹（全部/下载中/已完成/失败/已暂停）已内嵌分类子项，**不做独立的「分类」分区**（用户 2026-09-09 明确决定）；分类的新建/编辑走状态文件夹内分类子项的右键菜单。
- Rust 全部默认安全：禁止新增 `unsafe`（块 / fn / impl / extern）。唯一例外是 std 与现有直接依赖（`fs2` / `directories`）都没有安全封装的平台 FFI，且必须 `cfg` 门控 + `// SAFETY:` 注释 + 最小范围 + 安全外层函数 + RAII 守卫；`native/{api,protocol,daemon,server}` 与 `crates/*` 恒为零 `unsafe`。细则见 `rule://no-unsafe-in-rust`。
