use crate::exceptions::Result;
use crate::types::AuditEntry;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

#[derive(Debug, Clone)]
pub struct AuditLog {
    pool: Pool<Sqlite>,
}

impl AuditLog {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn append(&self, entry: &AuditEntry) -> Result<()> {
        let ts = entry.ts;
        sqlx::query(
            "INSERT INTO audit_entries (event, amount, description, band, ts, approved_by, denied_by, payment_intent_id, stripe_status, reason, error, recipe, recipe_result, recipe_error, receipt_fingerprint)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.event)
        .bind(entry.amount)
        .bind(&entry.description)
        .bind(entry.band.to_string())
        .bind(ts)
        .bind(entry.approved_by.as_deref())
        .bind(entry.denied_by.as_deref())
        .bind(entry.payment_intent_id.as_deref())
        .bind(entry.stripe_status.as_deref())
        .bind(entry.reason.as_deref())
        .bind(entry.error.as_deref())
        .bind(entry.recipe.as_deref())
        .bind(entry.recipe_result.as_deref())
        .bind(entry.recipe_error.as_deref())
        .bind(entry.receipt_fingerprint.as_deref())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn read_all(&self) -> Result<Vec<AuditEntryRow>> {
        let rows = sqlx::query_as::<_, AuditEntryRow>("SELECT * FROM audit_entries ORDER BY ts")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub async fn tail(&self, limit: i64) -> Result<Vec<AuditEntryRow>> {
        let rows = sqlx::query_as::<_, AuditEntryRow>(
            "SELECT * FROM audit_entries ORDER BY ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn total_spent(&self, autonomous_only: bool, approved_only: bool) -> Result<f64> {
        let mut query =
            String::from("SELECT COALESCE(SUM(amount), 0) FROM audit_entries WHERE event = 'executed'");
        if autonomous_only {
            query.push_str(" AND approved_by IS NULL");
        }
        if approved_only {
            query.push_str(" AND approved_by IS NOT NULL");
        }
        let total: Option<f64> = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
        Ok(total.unwrap_or(0.0))
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AuditEntryRow {
    pub id: i64,
    pub event: String,
    pub amount: f64,
    pub description: String,
    pub band: String,
    pub ts: chrono::DateTime<Utc>,
    pub approved_by: Option<String>,
    pub denied_by: Option<String>,
    pub payment_intent_id: Option<String>,
    pub stripe_status: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub recipe: Option<String>,
    pub recipe_result: Option<String>,
    pub recipe_error: Option<String>,
    pub receipt_fingerprint: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KillSwitchRow {
    pub id: i64,
    pub killed: bool,
    pub reason: String,
    pub by_operator: String,
    pub changed_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct KillSwitch {
    pool: Pool<Sqlite>,
}

impl KillSwitch {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn is_engaged(&self) -> Result<bool> {
        let row = sqlx::query_as::<_, KillSwitchRow>("SELECT * FROM kill_switch WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.killed)
    }

    pub async fn engage(&self, by: impl Into<String>, reason: impl Into<String>) -> Result<()> {
        let by = by.into();
        let reason = reason.into();
        sqlx::query(
            "INSERT INTO kill_switch (id, killed, reason, by_operator, changed_at)
             VALUES (1, 1, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET killed = 1, reason = excluded.reason, by_operator = excluded.by_operator, changed_at = excluded.changed_at",
        )
        .bind(&reason)
        .bind(&by)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn release(&self, by: impl Into<String>, reason: impl Into<String>) -> Result<()> {
        let by = by.into();
        let reason = reason.into();
        sqlx::query(
            "INSERT INTO kill_switch (id, killed, reason, by_operator, changed_at)
             VALUES (1, 0, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET killed = 0, reason = excluded.reason, by_operator = excluded.by_operator, changed_at = excluded.changed_at",
        )
        .bind(&reason)
        .bind(&by)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
