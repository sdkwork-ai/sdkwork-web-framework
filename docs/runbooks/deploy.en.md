# Runbook — sdkwork-web-framework deploy / upgrade / rollback (EN)

Environments: `development|test|staging|demo|production`. Commands run on the
local WSL host by default; append `--host ssh://[user@]host[:port]` for remote
targets. Image reference: `registry.sdkwork.com/apps/sdkwork-web-framework-standalone:0.1.0`
(tag from `sdkwork.app.config.json` → `release.currentVersion`).

> ℹ️ **Module shape**: this is an assembly-only module (it ships assembly crates but no standalone server binary),
> so the image build hook is marked not-applicable and fails closed; wire it per MODULE_BIN_SPEC.md §4.1 if a standalone
> gateway binary lands later. Sections 1 (install) and 2 (upgrade) are not executable until a container image exists.

## 1. Install (first time)

```bash
bin/docker-deploy.sh install --environment <development|test|staging|demo|production>
bin/docker-deploy.sh install --environment production --yes   # --yes is mandatory in production
```

install syncs the bundle to `/opt/deploy/sdkwork-web-framework/bundle`, loads the image, starts
instances and waits on the health gate (`/healthz`).

## 2. Upgrade

staging/demo/production capture a pre-change backup automatically (skip with
`--skip-backup`; the skip is recorded as evidence):

```bash
bin/docker-image.sh build
bin/docker-deploy.sh upgrade --environment staging --image-tag 0.1.0
```

## 3. Verify (release gate)

```bash
bin/docker-deploy.sh status --environment staging
bin/doctor.sh --environment staging          # aggregated diagnostics (9 checks)
```

## 4. Rollback

```bash
bin/docker-deploy.sh rollback --environment staging                  # previous ledger version
bin/docker-deploy.sh rollback --environment staging --to 0.1.0       # explicit version
```

Rollback is gated by `/healthz` on the management ports; a failed gate
auto-reverts and appends to `release-state/<env>/ledger.jsonl`. Migrations are
forward-only: across an incompatible schema the only recovery is a data
restore (backup-restore.md).

## 5. Retire

```bash
bin/docker-deploy.sh down --environment staging
bin/docker-deploy.sh stop    --environment staging   # stop (keeps containers and volumes; no repackage)
bin/docker-deploy.sh start   --environment staging   # start a stopped stack (embedded deps first)
bin/docker-deploy.sh restart --environment staging   # restart app instances only (deps stay up)
bin/docker-deploy.sh down --environment staging --purge --yes
```

## 6. Wiring prerequisites (implementation checklist)

1. This module has no standalone server binary yet (assembly-only). If the product decides on container delivery, land a
   standalone gateway crate first (mirror the family's `sdkwork-api-<module>-standalone-gateway`).
2. Once the binary exists: implement `bin/lib/module.sh` → `sdkwork_image_build`, and land `deployments/docker/bundle/` per
   OPERATIONS_SPEC.md §1.2 / DOCKER_SPEC.md §4.
3. Acceptance: `node ../sdkwork-specs/tools/check-operations-conformance.mjs --root .` all green.

<!-- generated: scaffold-module-runbooks.mjs -->
