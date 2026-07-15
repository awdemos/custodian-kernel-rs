use crate::exceptions::{CustodianError, Result};
use crate::types::{Band, Context};
use serde::{Deserialize, Serialize};

pub const VALID_APPROVAL_BACKENDS: [&str; 2] = ["twilio_verify", "none"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BandConfig {
    pub name: Band,
    pub max_spend: Option<f64>,
    pub requires_approval: bool,
    #[serde(default)]
    pub approval_backend: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub daily_envelope: Option<f64>,
    #[serde(default, alias = "band_after_task")]
    pub band_after_task: Option<Band>,
}

impl BandConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(max) = self.max_spend {
            if max < 0.0 {
                return Err(CustodianError::PolicyValidation(format!(
                    "band {}: max_spend must be >= 0, got {max}",
                    self.name
                )));
            }
        }
        if self.requires_approval && self.approval_backend.is_none() {
            return Err(CustodianError::PolicyValidation(format!(
                "band {}: requires_approval=true but no approval_backend set",
                self.name
            )));
        }
        if let Some(ref backend) = self.approval_backend {
            if !VALID_APPROVAL_BACKENDS.contains(&backend.as_str()) {
                return Err(CustodianError::PolicyValidation(format!(
                    "band {}: unknown approval_backend '{backend}'",
                    self.name
                )));
            }
        }
        if let Some(env) = self.daily_envelope {
            if env < 0.0 {
                return Err(CustodianError::PolicyValidation(format!(
                    "band {}: daily_envelope must be >= 0, got {env}",
                    self.name
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MarginsConfig {
    pub minimum_margin: Option<f64>,
    pub minimum_margin_pct: Option<f64>,
}

impl MarginsConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(m) = self.minimum_margin {
            if m < 0.0 {
                return Err(CustodianError::PolicyValidation(format!(
                    "margins.minimum_margin must be >= 0, got {m}"
                )));
            }
        }
        if let Some(p) = self.minimum_margin_pct {
            if !(0.0..=100.0).contains(&p) {
                return Err(CustodianError::PolicyValidation(format!(
                    "margins.minimum_margin_pct must be in [0, 100], got {p}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PoliciesConfig {
    #[serde(default)]
    pub no_self_dealing: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MatchCondition {
    pub skill: Option<String>,
    pub context_flag: Option<String>,
    #[serde(default)]
    pub context_flag_equals: Option<bool>,
    pub spend_estimate_gt: Option<f64>,
}

impl MatchCondition {
    pub fn matches(
&self, skill: Option<&str>, context: &Context, spend_estimate: Option<f64>) -> bool {
        if let Some(ref expected_skill) = self.skill {
            if skill != Some(expected_skill.as_str()) {
                return false;
            }
        }
        if let Some(ref flag) = self.context_flag {
            let expected = self.context_flag_equals.unwrap_or(true);
            let actual = context
                .get(flag)
                .map(|v| v.as_bool().unwrap_or(!expected))
                .unwrap_or(!expected);
            if actual != expected {
                return false;
            }
        }
        if let Some(threshold) = self.spend_estimate_gt {
            if !spend_estimate.is_some_and(|v| v > threshold) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Rule {
    #[serde(default)]
    pub order: i32,
    pub r#match: MatchCondition,
    pub assign_band: Band,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EscalationConfig {
    #[serde(default = "default_timeout")]
    pub timeout_seconds: i32,
    #[serde(default = "default_on_timeout")]
    pub on_timeout: String,
    #[serde(default)]
    pub retry_count: i32,
}

fn default_timeout() -> i32 {
    600
}

fn default_on_timeout() -> String {
    "deny".to_string()
}

impl Default for EscalationConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 600,
            on_timeout: "deny".to_string(),
            retry_count: 0,
        }
    }
}

impl EscalationConfig {
    pub fn validate(&self) -> Result<()> {
        if self.timeout_seconds <= 0 {
            return Err(CustodianError::PolicyValidation(
                "escalation.timeout_seconds must be positive".to_string(),
            ));
        }
        if self.on_timeout != "deny" && self.on_timeout != "retry" {
            return Err(CustodianError::PolicyValidation(format!(
                "escalation.on_timeout must be 'deny' or 'retry', got '{}'",
                self.on_timeout
            )));
        }
        if self.retry_count < 0 {
            return Err(CustodianError::PolicyValidation(
                "escalation.retry_count must be >= 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Policy {
    #[serde(default = "default_version")]
    pub version: String,
    pub default_band: Band,
    pub bands: std::collections::HashMap<Band, BandConfig>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub escalation: EscalationConfig,
    #[serde(default)]
    pub margins: Option<MarginsConfig>,
    #[serde(default)]
    pub policies: Option<PoliciesConfig>,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.version != "1.0" {
            return Err(CustodianError::PolicyValidation(format!(
                "unsupported policy version: {}",
                self.version
            )));
        }
        if !self.bands.contains_key(&self.default_band) {
            return Err(CustodianError::PolicyValidation(format!(
                "default_band '{}' is not defined in bands",
                self.default_band
            )));
        }
        for band_cfg in self.bands.values() {
            band_cfg.validate()?;
        }
        for rule in &self.rules {
            if !self.bands.contains_key(&rule.assign_band) {
                return Err(CustodianError::PolicyValidation(format!(
                    "rule assigns undefined band '{}'",
                    rule.assign_band
                )));
            }
        }
        self.escalation.validate()?;
        if let Some(ref margins) = self.margins {
            margins.validate()?;
        }
        for band_cfg in self.bands.values() {
            if let Some(ref after) = band_cfg.band_after_task {
                if !self.bands.contains_key(after) {
                    return Err(CustodianError::PolicyValidation(format!(
                        "band {}: band_after_task '{}' is not defined in bands",
                        band_cfg.name, after
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn band_for(
&self, skill: Option<&str>, context: &Context, spend_estimate: Option<f64>) -> Band {
        let mut ordered = self.rules.clone();
        ordered.sort_by_key(|a| a.order);
        for rule in &ordered {
            if rule.r#match.matches(skill, context, spend_estimate) {
                return rule.assign_band;
            }
        }
        self.default_band
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SkillManifest {
    pub name: String,
    pub band: Band,
    pub description: String,
}
