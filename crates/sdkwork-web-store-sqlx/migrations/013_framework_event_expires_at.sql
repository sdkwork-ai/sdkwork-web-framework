-- 013: framework_audit_event 和 framework_security_event 补充 expires_at 字段。
-- DATABASE_SPEC：审计/安全事件需 TTL 或归档策略。
-- 修复 HIGH：表无限增长，purge.rs 无对应清理逻辑。

ALTER TABLE framework_audit_event ADD COLUMN expires_at INTEGER;
ALTER TABLE framework_security_event ADD COLUMN expires_at INTEGER;

-- 为已有数据设置 90 天默认 TTL（从 created_at 计算）。
UPDATE framework_audit_event SET expires_at = created_at + 7776000 WHERE expires_at IS NULL;
UPDATE framework_security_event SET expires_at = created_at + 7776000 WHERE expires_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_framework_audit_expires ON framework_audit_event(expires_at);
CREATE INDEX IF NOT EXISTS idx_framework_security_event_expires ON framework_security_event(expires_at);
