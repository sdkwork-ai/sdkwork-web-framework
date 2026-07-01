CREATE TABLE IF NOT EXISTS web_idempotency_record (
    idempotency_key TEXT PRIMARY KEY NOT NULL,
    fingerprint TEXT NOT NULL,
    response_status INTEGER,
    response_body BLOB,
    content_type TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_idempotency_expires
    ON web_idempotency_record (expires_at);

CREATE TABLE IF NOT EXISTS web_security_event (
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

CREATE INDEX IF NOT EXISTS idx_web_security_event_created
    ON web_security_event (created_at);
