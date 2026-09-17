CREATE TABLE IF NOT EXISTS framework_tenant_runtime_profile (
    tenant_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    rate_limit_enabled INTEGER,
    max_content_length INTEGER,
    PRIMARY KEY (tenant_id, environment)
);

CREATE INDEX IF NOT EXISTS idx_framework_tenant_runtime_profile_tenant
    ON framework_tenant_runtime_profile (tenant_id);
