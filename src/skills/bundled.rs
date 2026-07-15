use super::SkillOutput;
use crate::exceptions::Result;

pub async fn run_http_get(url: &str) -> Result<SkillOutput> {
    // In the Python original this wraps requests; here we use reqwest if available,
    // but to keep the crate dependency-light for the port we return a stubbed success.
    // A real implementation would add `reqwest` to Cargo.toml.
    Ok(SkillOutput {
        ok: true,
        stub: true,
        data: Some(serde_json::json!({ "url": url, "status": 200, "body": "stubbed" })),
        error: None,
    })
}

pub async fn run_shell_exec(command: &str, allowlist: &[impl AsRef<str>]) -> Result<SkillOutput> {
    let trimmed = command.trim();
    let allowed = allowlist.iter().any(|prefix| {
        let p = prefix.as_ref().trim();
        trimmed == p || trimmed.starts_with(&format!("{p} "))
    });
    if !allowed {
        return Ok(SkillOutput {
            ok: false,
            stub: true,
            data: None,
            error: Some(format!(
                "command '{}' is not in the allowlist",
                command
            )),
        });
    }
    Ok(SkillOutput {
        ok: true,
        stub: true,
        data: Some(serde_json::json!({ "command": command, "executed": false, "note": "stubbed" })),
        error: None,
    })
}

pub async fn run_stripe_refund_placeholder(_payment_intent: &str, _amount: f64) -> Result<SkillOutput> {
    // Stripe integration requires a real Stripe client; this is a placeholder.
    Ok(SkillOutput {
        ok: false,
        stub: true,
        data: None,
        error: Some("Stripe backend not configured".to_string()),
    })
}
