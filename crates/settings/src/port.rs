//! 设置能力的宿主端口：由 app 注入单一 agent 会话与本机主题库，本 crate 只知道方法名、
//! JSON 与主题文件原文。

use std::{io, pin::Pin, time::SystemTime};

pub type PortFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, fluxdown_protocol::RpcErrorData>> + Send + 'static>>;

pub trait SettingsPort: Send + Sync {
    fn call(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> PortFuture<serde_json::Value>;
}

/// 主题库中一个主题的元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeInfo {
    /// 库内唯一 id（偏好里写作 `custom:<id>`）。
    pub id: String,
    /// 最后修改时间；宿主无法取得时为 `None`。
    pub modified: Option<SystemTime>,
}

/// 主题库中的一个主题：元数据 + 导入时的文件原文（未经改写）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTheme {
    pub info: ThemeInfo,
    pub text: String,
}

/// 本机主题库：导入的 GPUI 主题文件（`ThemeDocument` JSON 原文）按 id 存取。
///
/// 只同步选择值（`custom:<id>`），文件本身留在本机；实现由 app 提供（文件系统）。
/// 方法均为同步调用，文件很小；调用方可放到后台执行器。
pub trait ThemeLibrary: Send + Sync {
    /// 全部主题的元数据，按 id 升序；库目录不存在时为空。
    fn list(&self) -> io::Result<Vec<ThemeInfo>>;

    /// 读取一个主题的原文。
    fn load(&self, id: &str) -> io::Result<StoredTheme>;

    /// 原样保存 `text`，返回分配的 id：`preferred_id`（通常为 `meta.id`）清洗后可用则取之，
    /// 否则按时间戳生成；与已有主题冲突时加数字后缀，从不覆盖已有文件。
    fn save(&self, text: &str, preferred_id: Option<&str>) -> io::Result<String>;

    /// 删除一个主题；不存在视为成功。
    fn delete(&self, id: &str) -> io::Result<()>;
}
