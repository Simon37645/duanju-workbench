use serde::{Serialize, Serializer};

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("网络错误: {0}")]
    Http(#[from] reqwest::Error),
    #[error("未找到: {0}")]
    NotFound(String),
    #[error("参数错误: {0}")]
    Invalid(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("未打开项目")]
    NoProject,
    #[error("供应商错误: {0}")]
    Provider(String),
    #[error("已取消")]
    Canceled,
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Http(_) => "http",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Config(_) => "config",
            AppError::NoProject => "no_project",
            AppError::Provider(_) => "provider",
            AppError::Canceled => "canceled",
            AppError::Other(_) => "other",
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        AppError::Other(msg.into())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        AppError::Invalid(msg.into())
    }
}

/// 让错误能直接作为 tauri command 的返回类型序列化给前端。
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}
