CREATE TABLE IF NOT EXISTS web_cors_policy (
    tenant_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    allow_all_origins INTEGER NOT NULL DEFAULT 0,
    allowed_origins TEXT NOT NULL,
    allow_credentials INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (tenant_id, environment)
);

CREATE INDEX IF NOT EXISTS idx_web_cors_policy_tenant
    ON web_cors_policy (tenant_id);
