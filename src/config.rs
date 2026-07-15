use crate::exceptions::{CustodianError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CustodianConfig {
    pub state_dir: PathBuf,
    pub policy_path: PathBuf,
    pub database_url: String,
    pub pending_ttl_seconds: i64,
}

impl Default for CustodianConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl CustodianConfig {
    pub fn from_env() -> Self {
        Self {
            state_dir: path_env("CUSTODIAN_STATE_DIR", "./state"),
            policy_path: path_env("CUSTODIAN_POLICY_PATH", "./policy.yaml"),
            database_url: std::env::var("CUSTODIAN_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./state/custodian.db".to_string()),
            pending_ttl_seconds: int_env("CUSTODIAN_PENDING_TTL_SECONDS", 600),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.pending_ttl_seconds <= 0 {
            return Err(CustodianError::Config(
                "pending_ttl_seconds must be positive".to_string(),
            ));
        }
        std::fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }

    pub fn authority_file(&self) -> PathBuf {
        self.state_dir.join("authority.json")
    }

    pub fn pending_approval_file(&self) -> PathBuf {
        self.state_dir.join("pending_approval.json")
    }
}

fn path_env(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn int_env(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone)]
pub struct SecretConfig {
    pub stripe_secret_file: PathBuf,
    pub twilio_secret_file: PathBuf,
}

impl Default for SecretConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl SecretConfig {
    pub fn from_env() -> Self {
        Self {
            stripe_secret_file: path_env("CUSTODIAN_STRIPE_SECRET_FILE", "./secrets/stripe.env"),
            twilio_secret_file: path_env("CUSTODIAN_TWILIO_SECRET_FILE", "./secrets/twilio.env"),
        }
    }

    pub fn read_file(path: &Path) -> Result<String> {
        if !path.exists() {
            return Err(CustodianError::BackendConfiguration(format!(
                "secret file not found: {}",
                path.display()
            )));
        }
        Ok(std::fs::read_to_string(path)?)
    }
}
