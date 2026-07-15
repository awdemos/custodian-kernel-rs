use thiserror::Error;

#[derive(Debug, Error)]
pub enum CustodianError {
    #[error("policy validation error: {0}")]
    PolicyValidation(String),

    #[error("policy not found: {0}")]
    PolicyNotFound(String),

    #[error("backend error: {0}")]
    Backend(String),

    #[error("backend configuration error: {0}")]
    BackendConfiguration(String),

    #[error("audit write error: {0}")]
    AuditWrite(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("kill switch engaged -- all requests denied until an operator releases it")]
    KillSwitchEngaged,

    #[error("escalation required: {0}")]
    EscalationRequired(String),

    #[error("kernel denied: {0}")]
    Denied(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("migration error: {0}")]
    Migration(String),
}

impl From<sqlx::migrate::MigrateError> for CustodianError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        CustodianError::Migration(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CustodianError>;
