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
    #[error("代码已提交，读取评测结果失败：{0}。请刷新结果，勿重复提交")]
    EvaluationRead(#[source] Box<Error>),
    #[error("参数无效：{0}")]
    InvalidInput(&'static str),
    #[error("本地文件操作失败")]
    Io(#[source] std::io::Error),
    #[error("本地执行失败：{0}")]
    Execution(String),
    #[error("评测仍在进行，请稍后刷新结果；请勿重复提交")]
    EvaluationPending,
    #[error("保存或提交结果未知，请先刷新远端状态后再操作")]
    WriteUncertain,
    #[error("远端代码已变化，本地草稿已保留；请刷新题目并处理冲突后保存")]
    Conflict,
    #[error("系统凭据存储不可用或操作失败")]
    Vault,
    #[error("当前平台尚未接入持久化凭据存储")]
    UnsupportedVault,
    #[error("后台任务失败")]
    Background,
    #[error("响应超过大小限制")]
    ResponseTooLarge,
}
