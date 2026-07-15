use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernedReceipt {
    pub receipt_id: String,
    pub ts: f64,
    pub fn_name: String,
    pub band: String,
    pub amount: f64,
    pub description: String,
    pub verdict: String,
    pub reason: String,
    pub elapsed_ms: f64,
    pub output_hash: String,
    pub claim_proof: Option<String>,
    pub fingerprint: String,
}

impl GovernedReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        fn_name: impl Into<String>,
        band: impl Into<String>,
        amount: f64,
        description: impl Into<String>,
        verdict: impl Into<String>,
        reason: impl Into<String>,
        elapsed_ms: f64,
        output: &impl Serialize,
        claim_proof: Option<String>,
    ) -> Self {
        let receipt_id = Uuid::new_v4().to_string();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let fn_name_s: String = fn_name.into();
        let band_s: String = band.into();
        let verdict_s: String = verdict.into();
        let description_s: String = description.into();
        let reason_s: String = reason.into();
        let output_json = serde_json::to_string(output).unwrap_or_default();
        let output_hash = hex::encode(Sha256::digest(output_json.as_bytes()));
        let fingerprint = Self::fingerprint(
            &receipt_id,
            ts,
            &fn_name_s,
            &band_s,
            amount,
            &description_s,
            &verdict_s,
            &reason_s,
            elapsed_ms,
            &output_hash,
            claim_proof.as_deref(),
        );
        Self {
            receipt_id,
            ts,
            fn_name: fn_name_s,
            band: band_s,
            amount,
            description: description_s,
            verdict: verdict_s,
            reason: reason_s,
            elapsed_ms,
            output_hash,
            claim_proof,
            fingerprint,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fingerprint(
        receipt_id: &str,
        ts: f64,
        fn_name: &str,
        band: &str,
        amount: f64,
        description: &str,
        verdict: &str,
        reason: &str,
        elapsed_ms: f64,
        output_hash: &str,
        claim_proof: Option<&str>,
    ) -> String {
        let payload = format!(
            "{receipt_id}:{ts}:{fn_name}:{band}:{amount}:{description}:{verdict}:{reason}:{elapsed_ms}:{output_hash}:{claim_proof:?}"
        );
        hex::encode(Sha256::digest(payload.as_bytes()))
    }

    pub fn verify(&self) -> bool {
        let expected = Self::fingerprint(
            &self.receipt_id,
            self.ts,
            &self.fn_name,
            &self.band,
            self.amount,
            &self.description,
            &self.verdict,
            &self.reason,
            self.elapsed_ms,
            &self.output_hash,
            self.claim_proof.as_deref(),
        );
        self.fingerprint == expected
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
