use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Band {
    L0,
    #[default]
    L1,
    L2,
    L3,
    L4,
}

impl Band {
    pub fn rank(&self) -> u8 {
        match self {
            Band::L0 => 0,
            Band::L1 => 1,
            Band::L2 => 2,
            Band::L3 => 3,
            Band::L4 => 4,
        }
    }
}

impl std::fmt::Display for Band {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Band::L0 => "L0",
            Band::L1 => "L1",
            Band::L2 => "L2",
            Band::L3 => "L3",
            Band::L4 => "L4",
        })
    }
}

impl std::str::FromStr for Band {
    type Err = crate::exceptions::CustodianError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "L0" => Ok(Band::L0),
            "L1" => Ok(Band::L1),
            "L2" => Ok(Band::L2),
            "L3" => Ok(Band::L3),
            "L4" => Ok(Band::L4),
            _ => Err(crate::exceptions::CustodianError::PolicyValidation(format!(
                "unknown band: {s}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityState {
    pub band: Band,
    pub per_action_cap: f64,
    pub session_cap: f64,
    pub spent_this_session: f64,
}

impl AuthorityState {
    pub fn remaining_session_budget(&self) -> f64 {
        (self.session_cap - self.spent_this_session).max(0.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendRequest {
    pub amount: f64,
    pub description: String,
    pub recipe: Option<String>,
    pub to: Option<String>,
    pub message: Option<String>,
    pub revenue: Option<f64>,
    pub cost: Option<f64>,
    pub requester_agent_id: Option<String>,
    pub recipient_agent_id: Option<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub requested_at: DateTime<Utc>,
}

impl SpendRequest {
    pub fn new(amount: f64, description: impl Into<String>) -> Self {
        Self {
            amount,
            description: description.into(),
            recipe: None,
            to: None,
            message: None,
            revenue: None,
            cost: None,
            requester_agent_id: None,
            recipient_agent_id: None,
            requested_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Autonomous,
    EscalationRequired,
    Denied,
    ExecutionFailed,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::Autonomous => write!(f, "autonomous"),
            Verdict::EscalationRequired => write!(f, "escalation_required"),
            Verdict::Denied => write!(f, "denied"),
            Verdict::ExecutionFailed => write!(f, "execution_failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub verdict: Verdict,
    pub request: SpendRequest,
    pub reason: String,
    pub band: Band,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub amount: f64,
    pub description: String,
    pub reason: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

impl PendingApproval {
    pub fn is_expired(&self, ttl_seconds: i64) -> bool {
        let elapsed = Utc::now() - self.created_at;
        elapsed.num_seconds() > ttl_seconds
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KillSwitchState {
    pub killed: bool,
    pub reason: String,
    pub by: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub event: String,
    pub amount: f64,
    pub description: String,
    pub band: Band,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub ts: DateTime<Utc>,
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

const SECRET_SENTINELS: [&str; 21] = [
    "password",
    "passwd",
    "token",
    "secret",
    "credential",
    "mnemonic",
    "api_key",
    "apikey",
    "api-key",
    "ssh_key",
    "sshkey",
    "ssh-key",
    "private_key",
    "privatekey",
    "private-key",
    "access_key",
    "accesskey",
    "access-key",
    "auth_token",
    "bearer",
    "client_secret",
];

fn is_secret_key(key: &str) -> bool {
    let clean = key.trim().to_lowercase();
    if clean.ends_with("_file") || clean.ends_with("_dir") || clean.ends_with("_path") {
        return false;
    }
    SECRET_SENTINELS.iter().any(|s| {
        clean == *s || clean.starts_with(&format!("{s}_")) || clean.ends_with(&format!("_{s}"))
    })
}

fn sanitize_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(inner) => {
            let inner_map: HashMap<String, serde_json::Value> =
                inner.iter().map(|(kk, vv)| (kk.clone(), vv.clone())).collect();
            serde_json::Value::Object(sanitize_dict(&inner_map).into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sanitize_value).collect())
        }
        _ => v.clone(),
    }
}

pub fn sanitize_dict(d: &HashMap<String, serde_json::Value>) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::with_capacity(d.len());
    for (k, v) in d {
        if is_secret_key(k) {
            out.insert(k.clone(), serde_json::Value::String("[REDACTED]".to_string()));
        } else {
            out.insert(k.clone(), sanitize_value(v));
        }
    }
    out
}

pub type Context = HashMap<String, serde_json::Value>;
