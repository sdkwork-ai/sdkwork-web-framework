# SDKWork Web Framework Component Specs

Local standards index for `sdkwork-web-framework`. Root SDKWork standards remain authoritative; local specs narrow framework behavior only.

## Component

| Field | Value |
| --- | --- |
| Name | `sdkwork-web-framework` |
| Type | `rust-workspace` |
| Root | `sdkwork-web-framework` |
| Domain | `platform` |
| Capability | `web-framework` |
| Languages | `rust` |
| Maturity | M3 (`metadata.capabilityMaturity`) |

## Contract Manifest

- [component.spec.json](./component.spec.json) — machine-readable contract, verification gates, extension traits
- [WEB_FRAMEWORK_STANDARD.md](./WEB_FRAMEWORK_STANDARD.md) — L1 framework standard (narrows root specs)
- [web-framework-capability.matrix.json](./web-framework-capability.matrix.json) — capability maturity matrix

## Canonical Specs

| Spec | Purpose |
| --- | --- |
| [COMPONENT_SPEC.md](../../sdkwork-specs/COMPONENT_SPEC.md) | Local specs directory rules |
| [WEB_FRAMEWORK_SPEC.md](../../sdkwork-specs/WEB_FRAMEWORK_SPEC.md) | L0 web framework standard |
| [API_SPEC.md](../../sdkwork-specs/API_SPEC.md) | HTTP contract, request context |
| [WEB_BACKEND_SPEC.md](../../sdkwork-specs/WEB_BACKEND_SPEC.md) | Handler/service layering |
| [SECURITY_SPEC.md](../../sdkwork-specs/SECURITY_SPEC.md) | Security interceptor baseline |
| [OBSERVABILITY_SPEC.md](../../sdkwork-specs/OBSERVABILITY_SPEC.md) | Logs, metrics, correlation |
| [RUST_CODE_SPEC.md](../../sdkwork-specs/RUST_CODE_SPEC.md) | Rust implementation rules |
| [CODE_STYLE_SPEC.md](../../sdkwork-specs/CODE_STYLE_SPEC.md) | Cross-language style |
| [NAMING_SPEC.md](../../sdkwork-specs/NAMING_SPEC.md) | Naming conventions |
| [TEST_SPEC.md](../../sdkwork-specs/TEST_SPEC.md) | Verification expectations |
| [QUALITY_GATE_SPEC.md](../../sdkwork-specs/QUALITY_GATE_SPEC.md) | Release gate evidence |
| [DEPLOYMENT_SPEC.md](../../sdkwork-specs/DEPLOYMENT_SPEC.md) | Deployment profiles |

## Verification

```bash
scripts/verify.ps1   # Windows
scripts/verify.sh    # Unix
```

Commands mirror `component.spec.json` → `verification.commands`.

## Integration Docs

- [docs/architecture/tech/TECH-22-bootstrap-and-routing.md](../docs/architecture/tech/TECH-22-bootstrap-and-routing.md) — builder, manifest, service router
- [docs/architecture/tech/TECH-21-operations-runbook.md](../docs/architecture/tech/TECH-21-operations-runbook.md) — production operations
- [docs/architecture/tech/TECH-24-production-rollout-and-adoption.md](../docs/architecture/tech/TECH-24-production-rollout-and-adoption.md) — rollout SOP + adoption evidence
- [docs/architecture/tech/TECH-10-migration-from-appbase.md](../docs/architecture/tech/TECH-10-migration-from-appbase.md) — consumer migration

## Release Evidence

- `node scripts/collect-release-evidence.mjs` → `target/release-evidence/release-evidence.json`
- `specs/framework-adoption.evidence.json` — framework pathfinder adoptions (admin-server + PC console)
- `node scripts/validate-adoption-evidence.mjs <file>` — adoption JSON schema validation
