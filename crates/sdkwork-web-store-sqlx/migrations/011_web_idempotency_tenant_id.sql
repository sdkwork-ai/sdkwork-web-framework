-- 011: web_idempotency_record 补充 tenant_id 字段与索引。
-- DATABASE_SPEC §6.3：多租户表必须带 tenant_id。
-- 修复 HIGH：幂等键命名空间无法保证租户隔离。

ALTER TABLE web_idempotency_record ADD COLUMN tenant_id TEXT;

-- 兜底为平台共享 "0"。
UPDATE web_idempotency_record SET tenant_id = '0' WHERE tenant_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_web_idempotency_tenant
    ON web_idempotency_record(tenant_id);

CREATE INDEX IF NOT EXISTS idx_web_idempotency_tenant_expires
    ON web_idempotency_record(tenant_id, expires_at);
