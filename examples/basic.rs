use custodian_kernel::{
    audit::{AuditLog, KillSwitch},
    govern::{govern_call, GovernContext},
    policy::{minimal_policy, storage::NoopDailyEnvelopeStorage},
    setup_database,
    types::{Band, Context},
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = setup_database("sqlite:./state/custodian.db").await?;
    let audit = AuditLog::new(pool.clone());
    let kill = KillSwitch::new(pool.clone());

    let _policy = minimal_policy(Band::L2, 10.0);
    let ctx = GovernContext::<'_, NoopDailyEnvelopeStorage>::new(
        Some(Path::new("policy.yaml")),
        Some(Path::new("./state")),
    )
    .with_audit(&audit);

    let result = govern_call(
        "demo_charge",
        "Demo charge of $5",
        Band::L2,
        10.0,
        5.0,
        ctx,
        None,
        &Context::new(),
        kill.is_engaged().await?,
        || async move {
            Ok::<_, custodian_kernel::exceptions::CustodianError>(
                serde_json::json!({"ok": true, "id": "stub"}),
            )
        },
    )
    .await?;

    println!("verdict: {}", result.verdict);
    if let Some(ref value) = result.value {
        println!("output: {}", serde_json::to_string_pretty(value)?);
    }

    let receipt = result.receipt("within cap")?;
    println!("receipt verified: {}", receipt.verify());

    Ok(())
}
