//! 日志 shim —— 转发到 `fluxdown_engine::logger`。
//!
//! 实际实现已随其它零 rinf 耦合的模块迁移到 `fluxdown_engine`(引擎需要
//! 独立于 hub 记录自身日志)。此处保留 `crate::logger::*` 路径,使 hub 内
//! `updater.rs`/`actors/` 现有的 `use crate::logger::log_info;` 等导入继续工作。
pub use fluxdown_engine::logger::*;
