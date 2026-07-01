CREATE TABLE IF NOT EXISTS web_audit_event (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    tenant_id TEXT,
    user_id TEXT,
    api_surface TEXT NOT NULL,
    path TEXT NOT NULL,
    method TEXT NOT NULL,
    operation_id TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_web_audit_event_created
    ON web_audit_event (created_at);

CREATE INDEX IF NOT EXISTS idx_web_audit_event_request
    ON web_audit_event (request_id);

CREATE INDEX IF NOT EXISTS idx_web_audit_event_tenant
    ON web_audit_event (tenant_id);
