# Configuration templates

Safe, non-secret configuration templates for framework binaries and demos.

| Path | Purpose |
| --- | --- |
| `admin-server.env.example` | Non-secret env template for `sdkwork-web-admin-server` |
| `../apps/sdkwork-web-framework-pc/config/` | PC admin demo runtime config |

Server/container database URLs follow `RUNTIME_DIRECTORY_SPEC.md` and `sdkwork-database-config` env naming (`SDKWORK_WEB_FRAMEWORK_STORE_URL` in `WebFrameworkEnv`).

Environment keys are parsed by `crates/sdkwork-web-bootstrap/src/env_config.rs` (`WebFrameworkEnv::from_process_env`). Operational guidance: [docs/architecture/tech/TECH-21-operations-runbook.md](../docs/architecture/tech/TECH-21-operations-runbook.md).