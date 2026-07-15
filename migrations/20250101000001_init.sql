-- Audit ledger and kill switch state for custodian-kernel.

CREATE TABLE IF NOT EXISTS audit_entries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event       TEXT NOT NULL,
    amount      REAL NOT NULL DEFAULT 0,
    description TEXT NOT NULL DEFAULT '',
    band        TEXT NOT NULL,
    ts          DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    approved_by TEXT,
    denied_by   TEXT,
    payment_intent_id TEXT,
    stripe_status TEXT,
    reason      TEXT,
    error       TEXT,
    recipe      TEXT,
    recipe_result TEXT,
    recipe_error TEXT,
    receipt_fingerprint TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_ts ON audit_entries(ts);
CREATE INDEX IF NOT EXISTS idx_audit_event ON audit_entries(event);
CREATE INDEX IF NOT EXISTS idx_audit_band ON audit_entries(band);

CREATE TABLE IF NOT EXISTS kill_switch (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    killed      INTEGER NOT NULL DEFAULT 0,
    reason      TEXT NOT NULL DEFAULT '',
    by_operator TEXT NOT NULL DEFAULT '',
    changed_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO kill_switch (id, killed, reason, by_operator, changed_at)
VALUES (1, 0, '', '', datetime('now'));

CREATE TABLE IF NOT EXISTS daily_envelope_spent (
    band        TEXT PRIMARY KEY,
    amount      REAL NOT NULL DEFAULT 0,
    window_start DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
