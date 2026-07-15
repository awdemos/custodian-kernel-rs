use custodian_kernel::{
    audit::{AuditLog, KillSwitch},
    policy::{decide, minimal_policy, storage::NoopDailyEnvelopeStorage, schema::*},
    receipt::GovernedReceipt,
    session::CustodianSession,
    setup_database,
    types::{AuthorityState, Band, Context, SpendRequest, Verdict},
};

fn ctx() -> Context {
    Context::new()
}

fn state(band: Band, cap: f64, spent: f64) -> AuthorityState {
    AuthorityState {
        band,
        per_action_cap: cap,
        session_cap: cap * 10.0,
        spent_this_session: spent,
    }
}

#[tokio::test]
async fn self_approval_is_structurally_impossible() {
    // Regression: the same agent cannot be both requester and recipient when
    // no_self_dealing is enabled.
    let mut policy = minimal_policy(Band::L2, 100.0);
    policy.policies = Some(PoliciesConfig {
        no_self_dealing: true,
    });

    let mut req = SpendRequest::new(10.0, "self-payment");
    req.requester_agent_id = Some("agent-1".to_string());
    req.recipient_agent_id = Some("agent-1".to_string());

    let decision = decide(
        &req,
        &state(Band::L2, 100.0, 0.0),
        &policy,
        None,
        &ctx(),
        false,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();

    assert_eq!(decision.verdict, Verdict::Denied);
    assert!(decision.reason.contains("self_dealing"));
}

#[tokio::test]
async fn autonomous_under_cap() {
    let policy = minimal_policy(Band::L2, 100.0);
    let req = SpendRequest::new(50.0, "cheap api call");
    let decision = decide(
        &req,
        &state(Band::L2, 100.0, 0.0),
        &policy,
        None,
        &ctx(),
        false,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();
    assert_eq!(decision.verdict, Verdict::Autonomous);
}

#[tokio::test]
async fn escalation_over_cap() {
    let policy = minimal_policy(Band::L2, 10.0);
    let req = SpendRequest::new(50.0, "expensive api call");
    let decision = decide(
        &req,
        &state(Band::L2, 10.0, 0.0),
        &policy,
        None,
        &ctx(),
        false,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();
    assert_eq!(decision.verdict, Verdict::EscalationRequired);
    assert!(decision.reason.contains("exceeds"));
}

#[tokio::test]
async fn kill_switch_denies_everything() {
    let policy = minimal_policy(Band::L2, 100.0);
    let req = SpendRequest::new(1.0, "tiny request");
    let decision = decide(
        &req,
        &state(Band::L2, 100.0, 0.0),
        &policy,
        None,
        &ctx(),
        true,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();
    assert_eq!(decision.verdict, Verdict::Denied);
    assert!(decision.reason.contains("kill switch"));
}

#[tokio::test]
async fn receipt_verification_succeeds_and_tampering_fails() {
    let receipt = GovernedReceipt::build(
        "charge_customer",
        "L2",
        42.0,
        "monthly retainer",
        "autonomous",
        "within cap",
        12.5,
        &serde_json::json!({"ok": true, "id": "inv_123"}),
        None,
    );
    assert!(receipt.verify());

    let mut tampered = receipt.clone();
    tampered.amount = 100.0;
    assert!(!tampered.verify());
}

#[tokio::test]
async fn session_sub_session_cannot_exceed_parent_band() {
    let policy = minimal_policy(Band::L2, 100.0);
    let parent = CustodianSession::new(Band::L2, 100.0);
    let mut child = parent.sub_session(Band::L3, None);

    let req = SpendRequest::new(1.0, "child request");
    let result = child
        .request(req, &policy, Some(&NoopDailyEnvelopeStorage), None, &ctx(), false)
        .await
        .unwrap();
    assert_eq!(result.verdict, Verdict::Denied);
    assert!(result.reason.contains("exceeds parent ceiling"));
}

#[tokio::test]
async fn audit_log_and_kill_switch_persist_in_sqlite() {
    let pool = setup_database("sqlite::memory:").await.unwrap();
    let audit = AuditLog::new(pool.clone());
    let kill = KillSwitch::new(pool.clone());

    assert!(!kill.is_engaged().await.unwrap());

    kill.engage("operator-1", "emergency stop").await.unwrap();
    assert!(kill.is_engaged().await.unwrap());

    kill.release("operator-1", "all clear").await.unwrap();
    assert!(!kill.is_engaged().await.unwrap());

    let entry = custodian_kernel::types::AuditEntry {
        event: "executed".to_string(),
        amount: 25.0,
        description: "test spend".to_string(),
        band: Band::L2,
        ts: chrono::Utc::now(),
        approved_by: Some("op".to_string()),
        denied_by: None,
        payment_intent_id: None,
        stripe_status: None,
        reason: None,
        error: None,
        recipe: None,
        recipe_result: None,
        recipe_error: None,
        receipt_fingerprint: None,
    };
    audit.append(&entry).await.unwrap();

    let rows = audit.read_all().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].amount, 25.0);
}

#[tokio::test]
async fn margin_gate_denies_low_margin() {
    let mut policy = minimal_policy(Band::L2, 100.0);
    policy.margins = Some(MarginsConfig {
        minimum_margin: Some(5.0),
        minimum_margin_pct: None,
    });
    let mut req = SpendRequest::new(10.0, "low margin sale");
    req.revenue = Some(12.0);
    req.cost = Some(10.0);
    let decision = decide(
        &req,
        &state(Band::L2, 100.0, 0.0),
        &policy,
        None,
        &ctx(),
        false,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();
    assert_eq!(decision.verdict, Verdict::Denied);
    assert!(decision.reason.contains("margin"));
}
