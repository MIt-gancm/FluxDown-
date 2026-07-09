# FluxDown 粘性红线

仅收录 AGENTS.md 未覆盖的硬约束（其余规范见项目根 AGENTS.md，已自动加载）：

- 禁止未经用户明确要求执行 git commit / push / tag；推送 v* tag 会直接触发 GitHub Actions 全平台发布流水线，属不可逆操作。
