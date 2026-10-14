use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors deliberately exclude response bodies and credentials.
#[derive(Debug, Error)]
pub enum Error {
    #[error("登录会话无效或已过期，请重新导入 Cookie")]
    SessionExpired,
    #[error("Cookie 格式无效（第 {line} 行）")]
    InvalidCookie { line: usize },
    #[error("网络请求失败")]
    Transport(#[source] reqwest::Error),
    #[error("平台返回 HTTP {0}")]
    Http(u16),
    #[error("平台拒绝请求，状态码 {0}")]
    Platform(i64),
    #[error("平台时间校准失败")]
    Clock,
    #[error("响应结构不符合预期：{0}")]
    Response(&'static str),
    #[error("参数无效：{0}")]
    InvalidInput(&'static str),
    #[error("读取 Cookie 文件失败")]
    Io(#[source] std::io::Error),
    #[error("系统凭据存储不可用或操作失败")]
    Vault,
    #[error("当前平台尚未接入持久化凭据存储")]
    UnsupportedVault,
    #[error("后台任务失败")]
    Background,
    #[error("响应超过大小限制")]
    ResponseTooLarge,
}
