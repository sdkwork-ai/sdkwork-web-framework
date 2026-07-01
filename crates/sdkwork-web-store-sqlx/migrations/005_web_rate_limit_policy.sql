CREATE TABLE IF NOT EXISTS web_rate_limit_policy (
    tenant_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    tier_key TEXT NOT NULL DEFAULT 'default',
    max_requests INTEGER NOT NULL,
    window_secs INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (tenant_id, environment, tier_key)
);

CREATE INDEX IF NOT EXISTS idx_web_rate_limit_policy_tenant
    ON web_rate_limit_policy (tenant_id);
