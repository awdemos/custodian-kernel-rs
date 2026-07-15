use crate::exceptions::Result;
use crate::types::Band;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sqlx::{Pool, Sqlite};

#[async_trait]
pub trait DailyEnvelopeStorage: Send + Sync {
    async fn spent_today(&self, band: Band) -> Result<f64>;
    async fn record(&self, band: Band, amount: f64) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct SqliteDailyEnvelopeStorage {
    pool: Pool<Sqlite>,
    window: Duration,
}

impl SqliteDailyEnvelopeStorage {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self {
            pool,
            window: Duration::hours(24),
        }
    }

    pub fn with_window(mut self, hours: i64) -> Self {
        self.window = Duration::hours(hours);
        self
    }

    fn cutoff(&self) -> DateTime<Utc> {
        Utc::now() - self.window
    }
}

#[async_trait]
impl DailyEnvelopeStorage for SqliteDailyEnvelopeStorage {
    async fn spent_today(&self, band: Band) -> Result<f64> {
        let cutoff = self.cutoff();
        let total: Option<f64> = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM audit_entries WHERE band = ? AND ts >= ? AND event = 'executed'",
        )
        .bind(band.to_string())
        .bind(cutoff)
        .fetch_one(&self.pool)
        .await?;
        Ok(total.unwrap_or(0.0))
    }

    async fn record(&self, _band: Band, _amount: f64) -> Result<()> {
        // Audit entries are written by the audit log; this storage just queries them.
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct NoopDailyEnvelopeStorage;

#[async_trait]
impl DailyEnvelopeStorage for NoopDailyEnvelopeStorage {
    async fn spent_today(&self, _band: Band) -> Result<f64> {
        Ok(0.0)
    }

    async fn record(&self, _band: Band, _amount: f64) -> Result<()> {
        Ok(())
    }
}
