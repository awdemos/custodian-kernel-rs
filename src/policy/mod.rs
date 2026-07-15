pub mod schema;

use crate::exceptions::Result;
use crate::types::{AuthorityState, Band, Decision, SpendRequest, Verdict};

pub mod storage;

pub use schema::*;

pub async fn decide<S: storage::DailyEnvelopeStorage>(
    request: &SpendRequest,
    state: &AuthorityState,
    policy: &Policy,
    skill: Option<&str>,
    context: &crate::types::Context,
    killed: bool,
    ledger: Option<&S>,
) -> Result<Decision> {
    if killed {
        return Ok(Decision {
            verdict: Verdict::Denied,
            request: request.clone(),
            reason: "kill switch is engaged -- all requests denied until an operator releases it".to_string(),
            band: policy.default_band,
        });
    }

    let band = policy.band_for(skill, context, Some(request.amount));
    let mut band_cfg = policy
        .bands
        .get(&band)
        .cloned()
        .ok_or_else(|| crate::exceptions::CustodianError::PolicyValidation(format!("no band configuration found for '{band}'")))?;

    let mut effective_band = band;

    // Opt-in: auto-downgrade after task (band_after_task)
    if band_cfg.band_after_task.is_some() {
        if let Some(downgraded) = apply_autorank(state, &band_cfg) {
            effective_band = downgraded;
            band_cfg = policy
                .bands
                .get(&effective_band)
                .cloned()
                .unwrap_or(band_cfg);
        }
    }

    // Opt-in: daily envelope
    if let Some(envelope) = band_cfg.daily_envelope {
        if let Some(ledger) = ledger {
            let spent = ledger.spent_today(effective_band).await?;
            if spent + request.amount > envelope {
                return Ok(Decision {
                    verdict: Verdict::EscalationRequired,
                    request: request.clone(),
                    reason: format!(
                        "${:.2} would exceed band {effective_band} daily_envelope ${envelope:.2}",
                        request.amount
                    ),
                    band: effective_band,
                });
            }
        }
    }

    // Opt-in: margin gate
    if let Some(ref margins) = policy.margins {
        if let (Some(revenue), Some(cost)) = (request.revenue, request.cost) {
            if !check_margin(revenue, cost, margins) {
                return Ok(Decision {
                    verdict: Verdict::Denied,
                    request: request.clone(),
                    reason: format!(
                        "margin ${:.2} below minimum_margin ${:.2} or minimum_margin_pct {:.1}%",
                        revenue - cost,
                        margins.minimum_margin.unwrap_or(0.0),
                        margins.minimum_margin_pct.unwrap_or(0.0)
                    ),
                    band: effective_band,
                });
            }
        }
    }

    // Opt-in: self-dealing detector
    if let Some(ref policies) = policy.policies {
        if policies.no_self_dealing {
            if let (Some(req), Some(rec)) = (
                &request.requester_agent_id,
                &request.recipient_agent_id,
            ) {
                if !req.is_empty() && !rec.is_empty() && req == rec {
                    return Ok(Decision {
                        verdict: Verdict::Denied,
                        request: request.clone(),
                        reason: "self_dealing_detected: requester and recipient are the same agent".to_string(),
                        band: effective_band,
                    });
                }
            }
        }
    }

    let over_band_cap = band_cfg
        .max_spend
        .is_some_and(|max| request.amount > max);
    let over_session_cap = request.amount > state.remaining_session_budget();

    if band_cfg.requires_approval || over_band_cap || over_session_cap {
        let mut reasons = Vec::new();
        if band_cfg.requires_approval {
            reasons.push(format!("band {effective_band} always requires approval"));
        }
        if over_band_cap {
            reasons.push(format!(
                "${:.2} exceeds band {effective_band} max_spend ${:.2}",
                request.amount,
                band_cfg.max_spend.unwrap_or(0.0)
            ));
        }
        if over_session_cap {
            reasons.push(format!(
                "${:.2} exceeds remaining session budget ${:.2}",
                request.amount,
                state.remaining_session_budget()
            ));
        }
        return Ok(Decision {
            verdict: Verdict::EscalationRequired,
            request: request.clone(),
            reason: reasons.join("; "),
            band: effective_band,
        });
    }

    Ok(Decision {
        verdict: Verdict::Autonomous,
        request: request.clone(),
        reason: format!(
            "${:.2} within band {effective_band} (cap {}, remaining ${:.2})",
            request.amount,
            band_cfg
                .max_spend
                .map_or("unbounded".to_string(), |m| format!("${m:.2}")),
            state.remaining_session_budget()
        ),
        band: effective_band,
    })
}

fn apply_autorank(state: &AuthorityState, band_cfg: &schema::BandConfig) -> Option<Band> {
    // If a task has been completed (heuristic: any spend recorded), downgrade band.
    if state.spent_this_session > 0.0 {
        band_cfg.band_after_task
    } else {
        None
    }
}

fn check_margin(revenue: f64, cost: f64, margins: &schema::MarginsConfig) -> bool {
    if revenue <= 0.0 {
        return true;
    }
    let margin = revenue - cost;
    if let Some(min) = margins.minimum_margin {
        if margin < min {
            return false;
        }
    }
    if let Some(min_pct) = margins.minimum_margin_pct {
        let pct = (margin / revenue) * 100.0;
        if pct < min_pct {
            return false;
        }
    }
    true
}

pub fn load_policy(path: &std::path::Path) -> Result<Policy> {
    let content = std::fs::read_to_string(path)?;
    let policy: Policy = serde_yaml::from_str(&content)?;
    policy.validate()?;
    Ok(policy)
}

pub fn minimal_policy(band: Band, cap: f64) -> Policy {
    use std::collections::HashMap;
    let mut bands = HashMap::new();
    for b in [Band::L0, Band::L1, Band::L2, Band::L3, Band::L4] {
        bands.insert(
            b,
            BandConfig {
                name: b,
                max_spend: Some(cap),
                requires_approval: false,
                approval_backend: None,
                description: String::new(),
                daily_envelope: None,
                band_after_task: None,
            },
        );
    }
    Policy {
        version: "1.0".to_string(),
        default_band: band,
        bands,
        rules: Vec::new(),
        escalation: Default::default(),
        margins: None,
        policies: None,
    }
}
