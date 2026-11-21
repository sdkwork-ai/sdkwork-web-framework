-- sdkwork:migration
-- version: 0002
-- engine: postgres
-- module: framework
-- description: Renames the two BIGSERIAL sequences that still carry the retired
--   `web_` prefix. 0001_rename_table_prefix_to_framework renamed the eight tables
--   and their indexes and constraints, but not the sequences PostgreSQL creates
--   implicitly for `BIGSERIAL` columns. A database migrated from the `web_` era
--   therefore keeps web_audit_event_id_seq and web_security_event_id_seq while a
--   fresh install of the same baseline creates framework_audit_event_id_seq and
--   framework_security_event_id_seq; the two installs drift apart in a way no
--   drift rule inspects, because sequences are not part of the compared surface.
--   ALTER SEQUENCE ... RENAME TO preserves the sequence's ownership and current
--   value, and the owning column's DEFAULT keeps resolving because a regclass
--   reference is stored by object identity rather than by name.
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
    ARRAY['web_audit_event_id_seq', 'framework_audit_event_id_seq'],
    ARRAY['web_security_event_id_seq', 'framework_security_event_id_seq']
  ];
  renamed_count integer := 0;
  rec record;
BEGIN
  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    -- Fresh installs already create the framework_* sequence from the baseline.
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;

    -- Never drop a sequence that may already be in use: refuse if both names exist.
    IF to_regclass(pair[2]) IS NOT NULL THEN
      RAISE EXCEPTION
        'both % and % exist; refusing to rename (the % sequence may hold a live value)',
        pair[1], pair[2], pair[2];
    END IF;

    EXECUTE format('ALTER SEQUENCE %I RENAME TO %I', pair[1], pair[2]);
    renamed_count := renamed_count + 1;
  END LOOP;

  -- Self-check: no sequence in this schema may still carry the retired prefix.
  FOR rec IN
    SELECT c.relname AS name
    FROM pg_class c
    WHERE c.relkind = 'S'
      AND c.relnamespace = current_schema()::regnamespace
      AND c.relname LIKE 'web\_%'
  LOOP
    RAISE EXCEPTION 'sequence % still carries the retired web_ prefix', rec.name;
  END LOOP;

  RAISE NOTICE 'renamed % web_* sequence(s) to framework_*', renamed_count;
END $$;
