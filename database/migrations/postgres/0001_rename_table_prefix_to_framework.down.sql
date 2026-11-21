-- sdkwork:migration
-- version: 0001
-- engine: postgres
-- module: framework
-- description: Reverses 0001_rename_table_prefix_to_framework: renames the eight
--   sdkwork-web-framework platform infrastructure tables from `framework_` back to
--   `web_`, together with their primary keys, secondary indexes, and generated
--   *_not_null constraints.
-- reversible: false
-- rollback: restores the pre-0001 layout; requires the `web_` prefix to be free
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DO $$
DECLARE
  pair text[];
  pairs text[][] := ARRAY[
    ARRAY['framework_idempotency_record', 'web_idempotency_record'],
    ARRAY['framework_security_event', 'web_security_event'],
    ARRAY['framework_rate_limit_bucket', 'web_rate_limit_bucket'],
    ARRAY['framework_audit_event', 'web_audit_event'],
    ARRAY['framework_cors_policy', 'web_cors_policy'],
    ARRAY['framework_rate_limit_policy', 'web_rate_limit_policy'],
    ARRAY['framework_tenant_runtime_profile', 'web_tenant_runtime_profile'],
    ARRAY['framework_control_node', 'web_control_node']
  ];
  target_oid oid;
  rec record;
BEGIN
  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;
    IF to_regclass(pair[2]) IS NOT NULL THEN
      CONTINUE;
    END IF;

    EXECUTE format('ALTER TABLE %I RENAME TO %I', pair[1], pair[2]);
    target_oid := to_regclass(pair[2])::oid;

    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE 'framework\_%'
    LOOP
      EXECUTE format(
        'ALTER TABLE %I RENAME CONSTRAINT %I TO %I',
        pair[2], rec.name, replace(rec.name, 'framework_', 'web_')
      );
    END LOOP;

    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE 'framework\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, replace(rec.name, 'framework_', 'web_')
      );
    END LOOP;

    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE 'idx\_framework\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, 'idx_web_' || substr(rec.name, 15)
      );
    END LOOP;
  END LOOP;
END $$;
