use crate::exceptions::Result;
use crate::policy::schema::Policy;
use crate::policy::storage::DailyEnvelopeStorage;
use crate::policy::decide;
use crate::types::{Band, Context, SpendRequest, Verdict};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionResult {
    pub request: SpendRequest,
    pub verdict: Verdict,
    pub reason: String,
    pub audit_id: String,
}

impl SessionResult {
    pub fn ok(&self) -> bool {
        self.verdict == Verdict::Autonomous
    }
}

#[derive(Debug, Clone)]
pub struct CustodianSession {
    pub band: Band,
    pub cap: f64,
    pub session_cap: Option<f64>,
    pub daily_envelope: f64,
    pub policy_path: Option<std::path::PathBuf>,
    pub session_id: String,
    pub parent: Option<Box<CustodianSession>>,
    pub step: Option<String>,
    results: Vec<SessionResult>,
    spent: f64,
}

impl CustodianSession {
    pub fn new(band: Band, cap: f64) -> Self {
        Self {
            band,
            cap,
            session_cap: None,
            daily_envelope: cap * 5.0,
            policy_path: None,
            session_id: Uuid::new_v4().to_string().split('-').next().unwrap_or("sess").to_string(),
            parent: None,
            step: None,
            results: Vec::new(),
            spent: 0.0,
        }
    }

    pub fn with_policy_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.policy_path = Some(path.into());
        self
    }

    pub fn with_daily_envelope(mut self, envelope: f64) -> Self {
        self.daily_envelope = envelope;
        self
    }

    pub fn with_session_cap(mut self, cap: f64) -> Self {
        self.session_cap = Some(cap);
        self
    }

    pub fn with_step(mut self, step: impl Into<String>) -> Self {
        self.step = Some(step.into());
        self
    }

    fn audit_id(&self) -> String {
        format!("{}-{}", self.session_id, self.results.len())
    }

    pub async fn request<S: DailyEnvelopeStorage>(
        &mut self,
        request: SpendRequest,
        policy: &Policy,
        ledger: Option<&S>,
        skill: Option<&str>,
        context: &Context,
        killed: bool,
    ) -> Result<SessionResult> {
        // Child cannot exceed parent band ceiling.
        if let Some(ref parent) = self.parent {
            if self.band.rank() > parent.band.rank() {
                let r = SessionResult {
                    request: request.clone(),
                    verdict: Verdict::Denied,
                    reason: format!(
                        "sub-session band {} exceeds parent ceiling {}",
                        self.band, parent.band
                    ),
                    audit_id: self.audit_id(),
                };
                self.results.push(r.clone());
                return Ok(r);
            }
        }

        let state = crate::govern::authority_state(self.band, self.cap, self.session_cap, self.spent);
        let decision = decide(&request, &state, policy, skill, context, killed, ledger).await?;

        if decision.verdict == Verdict::Autonomous {
            self.spent += request.amount;
        }

        let r = SessionResult {
            request: request.clone(),
            verdict: decision.verdict,
            reason: decision.reason,
            audit_id: self.audit_id(),
        };
        self.results.push(r.clone());
        Ok(r)
    }

    pub fn sub_session(&self,
        band: Band,
        cap: Option<f64>,
    ) -> CustodianSession {
        let parent_cap = self.cap;
        let resolved_cap = cap.unwrap_or(parent_cap).min(parent_cap);
        CustodianSession {
            band,
            cap: resolved_cap,
            session_cap: self.session_cap.map(|sc| sc.min(resolved_cap * 10.0)),
            daily_envelope: self.daily_envelope,
            policy_path: self.policy_path.clone(),
            session_id: Uuid::new_v4().to_string().split('-').next().unwrap_or("sess").to_string(),
            parent: Some(Box::new(self.clone())),
            step: None,
            results: Vec::new(),
            spent: 0.0,
        }
    }

    pub fn log(&self) -> String {
        let mut lines = vec![format!(
            "CustodianSession {} — {} decisions, ${:.4} spent",
            self.session_id,
            self.results.len(),
            self.spent
        )];
        for r in &self.results {
            lines.push(format!(
                "  [{id}] {verdict:<22} ${amount:>8.2}  {desc:.50}",
                id = r.audit_id,
                verdict = r.verdict.to_string().to_uppercase(),
                amount = r.request.amount,
                desc = r.request.description
            ));
        }
        lines.join("\n")
    }

    pub fn summary(&self) -> serde_json::Value {
        use serde_json::json;
        let mut counts = std::collections::HashMap::new();
        for r in &self.results {
            *counts.entry(r.verdict.to_string()).or_insert(0usize) += 1;
        }
        json!({
            "session_id": self.session_id,
            "total": self.results.len(),
            "spent_usd": self.spent,
            "autonomous": counts.get("autonomous").copied().unwrap_or(0),
            "escalated": counts.get("escalation_required").copied().unwrap_or(0),
            "denied": counts.get("denied").copied().unwrap_or(0),
        })
    }
}
