-- 014: cors_policy / rate_limit_policy / tenant_runtime_profile 补充 version 字段。
-- DATABASE_SPEC §6.2：version 字段参与并发条件更新（乐观锁）。
-- 修复 HIGH：所有 upsert 为 last-write-wins，无冲突检测。

ALTER TABLE web_cors_policy ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE web_rate_limit_policy ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE web_tenant_runtime_profile ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
