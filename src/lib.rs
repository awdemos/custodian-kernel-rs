pub mod audit;
pub mod config;
pub mod exceptions;
pub mod govern;
pub mod policy;
pub mod receipt;
pub mod session;
pub mod skills;
pub mod types;

pub use audit::{AuditEntryRow, AuditLog, KillSwitch};
pub use config::{CustodianConfig, SecretConfig};
pub use exceptions::{CustodianError, Result};
pub use govern::{authority_state, govern_call, source_sha, GovernContext, GovernedResult};
pub use policy::schema::*;
pub use policy::storage::{DailyEnvelopeStorage, NoopDailyEnvelopeStorage, SqliteDailyEnvelopeStorage};
pub use policy::{decide, load_policy, minimal_policy};
pub use receipt::GovernedReceipt;
pub use session::{CustodianSession, SessionResult};
pub use skills::{run_http_get, run_shell_exec, run_stripe_refund_placeholder, SkillOutput};
pub use types::{AuditEntry, AuthorityState, Band, Context, Decision, KillSwitchState, PendingApproval, SpendRequest, Verdict, sanitize_dict};

use sqlx::{migrate::Migrator, Pool, Sqlite};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn setup_database(database_url: &str) -> Result<Pool<Sqlite>> {
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let pool = sqlx::SqlitePool::connect(database_url).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
