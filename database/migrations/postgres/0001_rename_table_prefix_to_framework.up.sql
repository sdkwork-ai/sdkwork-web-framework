-- sdkwork:migration
-- version: 0001
-- engine: postgres
-- module: framework
-- description: Renames the eight sdkwork-web-framework platform infrastructure tables
--   from the unregistered, doubly-claimed `web_` prefix to `framework_`, per
--   ADR-20260917-web-framework-table-prefix.md and DATABASE_SPEC §7. The tables are
--   framework_idempotency_record, framework_security_event, framework_rate_limit_bucket,
--   framework_audit_event, framework_cors_policy, framework_rate_limit_policy,
--   framework_tenant_runtime_profile, and framework_control_node.
--   PostgreSQL's `ALTER TABLE ... RENAME TO` does not rename the attached indexes or
--   constraints, so each renamed table's primary key, secondary indexes, and generated
--   *_not_null constraints are renamed explicitly. The rename is driven by an exact
--   table whitelist: `sdkwork-webserver` also owns tables under `web_` (for example
--   web_audit_log), and a prefix-scoped rename would corrupt them.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DO $$
DECLARE
  pair text[];
  pairs text[][] := ARRAY[
    ARRAY['web_idempotency_record', 'framework_idempotency_record'],
    ARRAY['web_security_event', 'framework_security_event'],
    ARRAY['web_rate_limit_bucket', 'framework_rate_limit_bucket'],
    ARRAY['web_audit_event', 'framework_audit_event'],
    ARRAY['web_cors_policy', 'framework_cors_policy'],
    ARRAY['web_rate_limit_policy', 'framework_rate_limit_policy'],
    ARRAY['web_tenant_runtime_profile', 'framework_tenant_runtime_profile'],
    ARRAY['web_control_node', 'framework_control_node']
  ];
  target_oid oid;
  existing_rows bigint;
  rec record;
BEGIN
  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    -- Nothing to rename: a fresh install already created framework_* from the baseline.
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;

    -- The lifecycle applies the baseline before migrations, and the baseline uses
    -- CREATE TABLE IF NOT EXISTS. On a database that still carries the legacy web_*
    -- tables, the baseline therefore materialises an empty framework_* twin. Drop that
    -- empty twin so the rename below can carry the legacy rows across. Refuse loudly
    -- if the twin is not empty rather than silently discarding data.
    IF to_regclass(pair[2]) IS NOT NULL THEN
      EXECUTE format('SELECT count(*) FROM %I', pair[2]) INTO existing_rows;
      IF existing_rows > 0 THEN
        RAISE EXCEPTION
          'both % and % exist and % holds % row(s); refusing to drop it',
          pair[1], pair[2], pair[2], existing_rows;
      END IF;
      EXECUTE format('DROP TABLE %I', pair[2]);
    END IF;

    EXECUTE format('ALTER TABLE %I RENAME TO %I', pair[1], pair[2]);
    target_oid := to_regclass(pair[2])::oid;

    -- Constraints: primary key plus the *_not_null entries PostgreSQL 17+ exposes.
    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE 'web\_%'
    LOOP
      EXECUTE format(
        'ALTER TABLE %I RENAME CONSTRAINT %I TO %I',
        pair[2], rec.name, replace(rec.name, 'web_', 'framework_')
      );
    END LOOP;

    -- Indexes whose name starts with web_ (the *_pkey objects).
    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE 'web\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, replace(rec.name, 'web_', 'framework_')
      );
    END LOOP;

    -- Indexes named idx_web_*.
    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE 'idx\_web\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, 'idx_framework_' || substr(rec.name, 9)
      );
    END LOOP;
  END LOOP;
END $$;
