use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub ok: bool,
    pub stub: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub mod bundled;
pub use bundled::{run_http_get, run_shell_exec, run_stripe_refund_placeholder};
