-- 010: web_security_event 补充 tenant_id 字段与索引。
-- SECURITY_SPEC §5.1 / DATABASE_SPEC §6.3：多租户表必须带 tenant_id 并建索引。
-- 修复 CRITICAL：安全事件无法按租户隔离/审计/导出。

ALTER TABLE web_security_event ADD COLUMN tenant_id TEXT;

-- 未鉴权请求的 tenant_id 兜底为平台共享 "0"。
UPDATE web_security_event SET tenant_id = '0' WHERE tenant_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_web_security_event_tenant
    ON web_security_event(tenant_id);

CREATE INDEX IF NOT EXISTS idx_web_security_event_tenant_created
    ON web_security_event(tenant_id, created_at);
