-- 012: web_rate_limit_bucket 补充 tenant_id 字段与索引。
-- DATABASE_SPEC §6.3：多租户表必须带 tenant_id。
-- 修复 HIGH：bucket_key 编码了 tenant 但 DB 层无 tenant_id 列，无法按租户清理/统计/下钻。

ALTER TABLE web_rate_limit_bucket ADD COLUMN tenant_id TEXT;

-- 兜底为平台共享 "0"。
UPDATE web_rate_limit_bucket SET tenant_id = '0' WHERE tenant_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_web_rate_limit_tenant
    ON web_rate_limit_bucket(tenant_id);

CREATE INDEX IF NOT EXISTS idx_web_rate_limit_tenant_expires
    ON web_rate_limit_bucket(tenant_id, expires_at);
