---
description: 拦截项目禁止执行的高副作用命令
scope: tool:bash
condition:
  - 'flutter\s+run\s+-d\s+windows'
  - 'cargo\s+test\s+--workspace'
  - 'git\s+push\s+\S*\s*v\d'
  - 'git\s+push\s+.*--tags'
repeatMode: after-gap
repeatGap: 2
---

该命令属于 FluxDown 项目禁令：

- Flutter 桌面运行/构建（桌面平台目标的 `flutter run` / `flutter build`）：Flutter 已仅限移动端，无桌面 runner。改用 `flutter analyze` / `flutter test` / `cargo check -p <crate>` 验证。
- `cargo test --workspace`：禁止全量跑。用 `cargo nextest run -p <crate> <filter>` 或 `cargo test -p <crate> -- <filter>` 精准跑。静态门禁不同：使用 `cargo clippy --workspace --exclude fluxdown_server --all-targets -- -D warnings`，含测试目标，冻结的 `native/server` 不构建。
- 推送 `v*` tag 会触发 GitHub Actions 全平台发布流水线，必须由用户明确要求后执行。

请换用合规命令后继续。
