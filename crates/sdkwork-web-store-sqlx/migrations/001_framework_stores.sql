CREATE TABLE IF NOT EXISTS framework_idempotency_record (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    fingerprint TEXT NOT NULL,
    response_status INTEGER,
    response_body BLOB,
    content_type TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_framework_idempotency_expires
    ON framework_idempotency_record (expires_at);

CREATE TABLE IF NOT EXISTS framework_security_event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    request_id TEXT,
    path TEXT NOT NULL,
    method TEXT NOT NULL,
    api_surface TEXT NOT NULL,
    origin TEXT,
    detail TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_framework_security_event_created
    ON framework_security_event (created_at);
