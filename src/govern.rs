use crate::audit::AuditLog;
use crate::config::CustodianConfig;
use crate::exceptions::Result;
use crate::policy::schema::Policy;
use crate::policy::storage::{self};
use crate::policy::load_policy;
use crate::receipt::GovernedReceipt;
use crate::types::{AuditEntry, AuthorityState, Band, Context, Decision, SpendRequest, Verdict};
use sha2::{Digest, Sha256};
use std::path::Path;
use uuid::Uuid;

pub fn authority_state(
    band: Band,
    per_action_cap: f64,
    session_cap: Option<f64>,
    spent_this_session: f64,
) -> AuthorityState {
    AuthorityState {
        band,
        per_action_cap,
        session_cap: session_cap.unwrap_or(per_action_cap * 10.0),
        spent_this_session,
    }
}

pub struct GovernContext<'a, S: storage::DailyEnvelopeStorage = storage::NoopDailyEnvelopeStorage> {
    pub policy_path: Option<&'a Path>,
    pub state_dir: Option<&'a Path>,
    pub session_cap: Option<f64>,
    pub spent_this_session: f64,
    pub ledger: Option<&'a S>,
    pub audit: Option<&'a AuditLog>,
}

impl<'a, S: storage::DailyEnvelopeStorage> GovernContext<'a, S> {
    pub fn new(policy_path: Option<&'a Path>, state_dir: Option<&'a Path>) -> Self {
        Self {
            policy_path,
            state_dir,
            session_cap: None,
            spent_this_session: 0.0,
            ledger: None,
            audit: None,
        }
    }

    pub fn with_session_cap(mut self, cap: f64) -> Self {
        self.session_cap = Some(cap);
        self
    }

    pub fn with_spent_this_session(mut self, spent: f64) -> Self {
        self.spent_this_session = spent;
        self
    }

    pub fn with_ledger(mut self, ledger: &'a S) -> Self {
        self.ledger = Some(ledger);
        self
    }

    pub fn with_audit(mut self, audit: &'a AuditLog) -> Self {
        self.audit = Some(audit);
        self
    }
}

pub struct GovernedResult<T> {
    pub value: Option<T>,
    pub verdict: Verdict,
    pub audit_id: String,
    pub band: Band,
    pub amount: f64,
    pub description: String,
    pub fn_name: String,
    pub elapsed_ms: f64,
    pub claim_proof: Option<String>,
}

impl<T: serde::Serialize> GovernedResult<T> {
    pub fn receipt(&self, reason: impl Into<String>) -> Result<GovernedReceipt> {
        Ok(GovernedReceipt::build(
            &self.fn_name,
            self.band.to_string(),
            self.amount,
            &self.description,
            self.verdict.to_string(),
            reason,
            self.elapsed_ms,
            &self.value,
            self.claim_proof.clone(),
        ))
    }
}

pub async fn evaluate<S: storage::DailyEnvelopeStorage>(
    request: &SpendRequest,
    state: &AuthorityState,
    ctx: &GovernContext<'_, S>,
    skill: Option<&str>,
    context: &Context,
    killed: bool,
) -> Result<Decision> {
    let policy = resolve_policy(ctx.policy_path).await?;
    crate::policy::decide(request, state, &policy, skill, context, killed, ctx.ledger).await
}

fn audit_event(
    event: impl Into<String>,
    amount: f64,
    description: impl Into<String>,
    band: Band,
    reason: Option<String>,
    error: Option<String>,
    receipt_fingerprint: Option<String>,
) -> AuditEntry {
    AuditEntry {
        event: event.into(),
        amount,
        description: description.into(),
        band,
        ts: chrono::Utc::now(),
        approved_by: None,
        denied_by: None,
        payment_intent_id: None,
        stripe_status: None,
        reason,
        error,
        recipe: None,
        recipe_result: None,
        recipe_error: None,
        receipt_fingerprint,
    }
}

async fn resolve_policy(
    policy_path: Option<&Path>,
) -> Result<Policy> {
    let cfg = CustodianConfig::from_env();
    let path = policy_path
        .map(|p| p.to_path_buf())
        .or_else(|| cfg.policy_path.exists().then(|| cfg.policy_path.clone()));

    if let Some(path) = path {
        return load_policy(&path);
    }
    Err(crate::exceptions::CustodianError::PolicyNotFound(
        "no policy file specified and CUSTODIAN_POLICY_PATH does not exist".to_string(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn govern_call<F, Fut, T, S: storage::DailyEnvelopeStorage>(
    fn_name: impl Into<String>,
    description: impl Into<String>,
    band: Band,
    cap: f64,
    amount: f64,
    ctx: GovernContext<'_, S>,
    skill: Option<&str>,
    context: &Context,
    killed: bool,
    call: F,
) -> Result<GovernedResult<T>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
    T: serde::Serialize,
{
    let fn_name = fn_name.into();
    let description = description.into();
    let request = SpendRequest::new(amount, &description);
    let state = authority_state(band, cap, ctx.session_cap, ctx.spent_this_session);
    let decision = evaluate(&request, &state, &ctx, skill, context, killed).await?;
    let audit = ctx.audit;

    if decision.verdict == Verdict::Denied {
        if let Some(audit) = audit {
            audit
                .append(&audit_event(
                    "denied",
                    amount,
                    description.clone(),
                    decision.band,
                    Some(decision.reason.clone()),
                    None,
                    None,
                ))
                .await?;
        }
        return Ok(GovernedResult {
            value: None,
            verdict: Verdict::Denied,
            audit_id: Uuid::new_v4().to_string(),
            band: decision.band,
            amount,
            description,
            fn_name,
            elapsed_ms: 0.0,
            claim_proof: None,
        });
    }

    if decision.verdict == Verdict::EscalationRequired {
        if let Some(audit) = audit {
            audit
                .append(&audit_event(
                    "escalation_required",
                    amount,
                    description.clone(),
                    decision.band,
                    Some(decision.reason.clone()),
                    None,
                    None,
                ))
                .await?;
        }
        return Ok(GovernedResult {
            value: None,
            verdict: Verdict::EscalationRequired,
            audit_id: Uuid::new_v4().to_string(),
            band: decision.band,
            amount,
            description,
            fn_name,
            elapsed_ms: 0.0,
            claim_proof: None,
        });
    }

    let t0 = tokio::time::Instant::now();
    let value = match call().await {
        Ok(v) => v,
        Err(e) => {
            if let Some(audit) = audit {
                audit
                    .append(&audit_event(
                        "execution_failed",
                        amount,
                        description.clone(),
                        decision.band,
                        Some(decision.reason.clone()),
                        Some(e.to_string()),
                        None,
                    ))
                    .await?;
            }
            return Ok(GovernedResult {
                value: None,
                verdict: Verdict::ExecutionFailed,
                audit_id: Uuid::new_v4().to_string(),
                band: decision.band,
                amount,
                description,
                fn_name,
                elapsed_ms: 0.0,
                claim_proof: None,
            });
        }
    };
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let receipt = GovernedReceipt::build(
        &fn_name,
        decision.band.to_string(),
        amount,
        &description,
        Verdict::Autonomous.to_string(),
        &decision.reason,
        elapsed_ms,
        &value,
        None,
    );

    if let Some(audit) = audit {
        audit
            .append(&audit_event(
                "executed",
                amount,
                description.clone(),
                decision.band,
                Some(decision.reason.clone()),
                None,
                Some(receipt.fingerprint.clone()),
            ))
            .await?;
    }

    Ok(GovernedResult {
        value: Some(value),
        verdict: Verdict::Autonomous,
        audit_id: Uuid::new_v4().to_string(),
        band: decision.band,
        amount,
        description,
        fn_name,
        elapsed_ms,
        claim_proof: None,
    })
}

pub fn source_sha(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| hex::encode(Sha256::digest(&bytes)))
}
