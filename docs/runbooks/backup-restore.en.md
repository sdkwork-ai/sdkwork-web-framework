# Runbook — sdkwork-web-framework backup & restore (EN)

## 1. Backup

```bash
bin/backup.sh create --environment production            # config + database + volumes, sha256 checksummed
bin/backup.sh list   --environment production
bin/backup.sh verify --environment production            # verify the latest set
```

Backup sets live on the target host under `/opt/deploy/sdkwork-web-framework/backups/`.
RPO: daily in production plus before every upgrade; RTO: production restore
completes within 4 hours.

## 2. Restore (destructive, requires --yes)

```bash
bin/backup.sh restore --environment production --set <set-name> --yes
bin/docker-deploy.sh install --environment production     # bring the stack back up after restore
```

## 3. Drill

Once per quarter, perform a real restore into a scratch environment (not just
`verify`).

## 6. Wiring prerequisites (implementation checklist)

1. This module has no standalone server binary yet (assembly-only). If the product decides on container delivery, land a
   standalone gateway crate first (mirror the family's `sdkwork-api-<module>-standalone-gateway`).
2. Once the binary exists: implement `bin/lib/module.sh` → `sdkwork_image_build`, and land `deployments/docker/bundle/` per
   OPERATIONS_SPEC.md §1.2 / DOCKER_SPEC.md §4.
3. Acceptance: `node ../sdkwork-specs/tools/check-operations-conformance.mjs --root .` all green.

<!-- generated: scaffold-module-runbooks.mjs -->
