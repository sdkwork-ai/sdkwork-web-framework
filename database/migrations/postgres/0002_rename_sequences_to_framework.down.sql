-- sdkwork:migration
-- version: 0002
-- engine: postgres
-- module: framework
-- description: Rollback of 0002_rename_sequences_to_framework: restores the two
--   BIGSERIAL sequence names to the retired `web_` prefix so a pre-Amendment-2
--   database can be reproduced. Values and ownership are preserved; the owning
--   columns' DEFAULT expressions follow the rename because they reference the
--   sequence by object identity.
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
    ARRAY['framework_audit_event_id_seq', 'web_audit_event_id_seq'],
    ARRAY['framework_security_event_id_seq', 'web_security_event_id_seq']
  ];
  renamed_count integer := 0;
BEGIN
  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;

    IF to_regclass(pair[2]) IS NOT NULL THEN
      RAISE EXCEPTION
        'both % and % exist; refusing to rename (the % sequence may hold a live value)',
        pair[1], pair[2], pair[2];
    END IF;

    EXECUTE format('ALTER SEQUENCE %I RENAME TO %I', pair[1], pair[2]);
    renamed_count := renamed_count + 1;
  END LOOP;

  RAISE NOTICE 'restored % framework_* sequence(s) to web_*', renamed_count;
END $$;
