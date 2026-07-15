# custodian-kernel (Rust port)

A Rust implementation of the [custodian-kernel](https://github.com/KeyArgo/custodian-kernel) authority-and-spend kernel for AI agents.

> **Disclaimer:** This is beta software. Use it at your own risk. It is not a substitute for financial, legal, or safety review, and the maintainers assume no liability for decisions made by agents governed by this crate.

The model proposes. The kernel decides. The ledger records. The kill switch stops.

## What it does

This crate enforces spend and authority rules *outside* the agent's own process. An agent (or any caller) submits a `SpendRequest`. The kernel evaluates it against a loaded policy, current authority state, daily spend envelope, kill-switch state, and opt-in gates such as margin or self-dealing checks. It returns one of three verdicts:

- `Autonomous` — execute now, no human involved.
- `EscalationRequired` — over a cap or band rule; human approval needed.
- `Denied` — kill switch engaged, self-dealing detected, margin too low, or policy explicitly denies.

Every executed, denied, escalated, or failed action can be written to an append-only SQLite audit log. Each autonomous action can produce a SHA-256 receipt whose fingerprint covers the function name, band, amount, description, verdict, reason, timing, output hash, and claim proof.

## Project status

This is a faithful port of the Python kernel's **core** model, not a complete reproduction. The public API is stable enough to build on, and the pieces omitted are deliberately left as well-defined seams so they can be added later without redesign.

### What is ported

- `Band` / `AuthorityState` / `SpendRequest` / `Decision` / `Verdict`
- YAML policy loading and validation (`Policy`, `BandConfig`, `Rule`, `MatchCondition`, `EscalationConfig`)
- Policy evaluation: per-band caps, session caps, daily envelope, margin gate, self-dealing detection, auto-downgrade (`band_after_task`), rule-based band assignment, fail-closed missing-band handling
- `GovernContext` / `govern_call` wrapper for executing arbitrary async work under kernel control
- `CustodianSession` and sub-sessions with parent-band ceiling enforcement
- SQLite-backed `AuditLog` and `KillSwitch` with embedded migrations
- `GovernedReceipt` with SHA-256 fingerprint and `verify()`
- Representative bundled skill stubs (`http_get`, `shell_exec`, `stripe_refund_placeholder`)
- Typed error hierarchy

### What is intentionally excluded (for now)

- **Human escalation backends** — e.g. Twilio Verify SMS approval. The original ships a `twilio_verify` backend that sends a code to an operator's phone and confirms it before allowing an escalated spend. That backend is **not implemented here**, but it is easy to add: implement the `ApprovalBackend` trait, wire it into `govern_call`'s `EscalationRequired` branch, and read `band_cfg.approval_backend` from the policy. The policy schema already reserves `approval_backend: "twilio_verify" | "none"`.
- **102 bundled skills** — only three stubbed examples exist. Adding a real skill means implementing its executor and declaring a `band`/`description` in the registry.
- **Claim-verification packs** — the deterministic claim verifier (`CONTRADICTED` / `VERIFIED` / `UNVERIFIABLE`) from the original is not ported.
- **ASGI middleware** — the Python `CustodianMiddleware` for FastAPI/Flask/Starlette is not ported.
- **State persistence beyond SQLite** — the original persisted `authority.json` and `pending_approval.json` on disk. This port keeps session state in memory and the audit/kill-switch state in SQLite.

## Quick start

```bash
cd custodian-kernel-rs
cargo test              # 8 representative tests, all passing
cargo run --example basic
```

`setup_database` and `CustodianConfig::validate` create the necessary directories automatically.

## Configuration

Environment variables (all optional):

| Variable | Default | Purpose |
|----------|---------|---------|
| `CUSTODIAN_STATE_DIR` | `./state` | Directory for local state (mostly advisory today) |
| `CUSTODIAN_POLICY_PATH` | `./policy.yaml` | YAML policy file |
| `CUSTODIAN_DATABASE_URL` | `sqlite:./state/custodian.db` | SQLite database URL |
| `CUSTODIAN_PENDING_TTL_SECONDS` | `600` | Pending approval TTL (reserved for future backend) |

## Policy YAML

```yaml
version: "1.0"
default_band: L2
bands:
  L0:
    name: L0
    max_spend: 0
    requires_approval: false
    description: "read-only"
  L1:
    name: L1
    max_spend: 5.00
    requires_approval: false
  L2:
    name: L2
    max_spend: 50.00
    requires_approval: false
    daily_envelope: 200.00
  L3:
    name: L3
    max_spend: 500.00
    requires_approval: true
    approval_backend: none
    description: "always escalates"
  L4:
    name: L4
    max_spend: 1000.00
    requires_approval: true
    approval_backend: none
rules:
  - order: 0
    match:
      skill: stripe-refund
    assign_band: L3
margins:
  minimum_margin: 5.00
  minimum_margin_pct: 20.0
policies:
  no_self_dealing: true
escalation:
  timeout_seconds: 600
  on_timeout: deny
```

## Usage

### Evaluate a request directly

```rust
use custodian_kernel::{
    policy::{decide, minimal_policy, storage::NoopDailyEnvelopeStorage},
    types::{Band, SpendRequest, AuthorityState},
};

#[tokio::main]
async fn main() {
    let policy = minimal_policy(Band::L2, 50.0);
    let state = AuthorityState {
        band: Band::L2,
        per_action_cap: 50.0,
        session_cap: 500.0,
        spent_this_session: 0.0,
    };
    let request = SpendRequest::new(25.0, "api call");

    let decision = decide(
        &request,
        &state,
        &policy,
        None,
        &Default::default(),
        false,
        Some(&NoopDailyEnvelopeStorage),
    )
    .await
    .unwrap();

    println!("{:?} — {}", decision.verdict, decision.reason);
}
```

### Wrap a call with `govern_call`

```rust
use custodian_kernel::{
    audit::AuditLog,
    govern::{govern_call, GovernContext},
    policy::{minimal_policy, storage::NoopDailyEnvelopeStorage},
    setup_database,
    types::{Band, Context},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = setup_database("sqlite:./state/custodian.db").await?;
    let audit = AuditLog::new(pool);

    let ctx = GovernContext::<'_, NoopDailyEnvelopeStorage>::new(None, None)
        .with_audit(&audit);

    let result = govern_call(
        "charge_customer",
        "Charge customer",
        Band::L2,
        50.0,
        25.0,
        ctx,
        None,
        &Context::new(),
        false,
        || async move { Ok(serde_json::json!({"id": "inv_123"})) },
    )
    .await?;

    println!("verdict: {}", result.verdict);
    Ok(())
}
```

### Use a session

```rust
use custodian_kernel::{
    policy::{minimal_policy, storage::NoopDailyEnvelopeStorage},
    session::CustodianSession,
    types::{Band, Context, SpendRequest},
};

#[tokio::main]
async fn main() {
    let policy = minimal_policy(Band::L2, 50.0);
    let mut session = CustodianSession::new(Band::L2, 50.0);

    let r = session
        .request(
            SpendRequest::new(25.0, "step 1"),
            &policy,
            Some(&NoopDailyEnvelopeStorage),
            None,
            &Context::new(),
            false,
        )
        .await
        .unwrap();

    assert!(r.ok());
    println!("{}", session.log());
}
```

## Architecture notes

- **Tokio + SQLx**: async runtime with SQLite for audit and kill-switch persistence.
- **Migrations**: embedded in `migrations/20250101000001_init.sql`, applied automatically by `setup_database`.
- **Seams**:
  - `DailyEnvelopeStorage` is the seam for spend-window queries; `SqliteDailyEnvelopeStorage` and `NoopDailyEnvelopeStorage` are the two shipped adapters.
  - `govern_call` is the seam for wrapping arbitrary agent actions.
  - Policy loading is a seam; `load_policy` validates the full YAML schema before returning.

## Known gaps and divergences from the Python kernel

1. **Twilio / human escalation backend** is not implemented. The `EscalationRequired` branch in `govern_call` returns without contacting any backend. Adding Twilio Verify is the most natural next step.
2. **Session spend is in-memory only**; the original persisted `authority.json`. Restarting the process resets the session budget.
3. **Autorank** was simplified: the port downgrades on any recorded session spend rather than per-agent with a TTL.

## Testing

```bash
cargo test
```

Current tests cover:

- Self-approval regression
- Autonomous / escalation / denied verdicts
- Kill switch
- Receipt tamper detection
- Sub-session band ceiling
- SQLite audit log and kill-switch persistence
- Margin gate

## License

MIT
