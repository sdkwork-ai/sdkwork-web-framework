# Schema Registry Framework Standard

- Version: 1.0
- Scope: `sdkwork-web-framework` schema registry composition implementation
- Status: active
- Authority: narrows `../sdkwork-specs/SCHEMA_REGISTRY_SPEC.md`; does not contradict root specs
- Related: `../sdkwork-specs/DATABASE_FRAMEWORK_SPEC.md`, `../sdkwork-specs/API_SPEC.md`

## 1. Purpose

This standard defines the executable profile for compositional schema registry loading inside `sdkwork-web-framework`.

Applications compose owner registries and local overlays through one implementation surface. They `MUST NOT` fork merge logic locally.

## 2. Crate and Tool Surfaces

| Artifact | Responsibility |
| --- | --- |
| `crates/sdkwork-web-schema-registry` | Deterministic registry composition library |
| `crates/sdkwork-web-schema-registry` binary `sdkwork-schema-registry` | CLI for compose/check/materialize |
| `tools/schema_registry/` | Python composer for application guardians and manifest tools |

Rules:

- Business repositories `MAY` depend on the Rust crate for tests and build-time validation.
- Application Python tools `MUST` import from `tools/schema_registry/` via workspace-relative path, not copy the module.
- The framework `MUST NOT` depend on any business repository registry content.

## 3. Supported Composition Kinds

| Kind | Entry file | Output |
| --- | --- | --- |
| `tables` | `docs/schema-registry/<app>.tables.yaml` | Effective table registry YAML |
| `frontend-contracts` | `docs/schema-registry/frontend-field-contracts/index.yaml` | Compiled frontend contract YAML |

Planned:

- Dependency frontend contract import by route reference
- OpenAPI operation overlay composition

## 4. Dependency Resolution

The composer resolves registry dependencies in this order:

1. Explicit `registry_dependencies` on the assembly registry
2. Else `database/database.manifest.json#modules[]`
3. Else no dependencies

Each dependency entry resolves:

- `locator` relative to the application root
- `registry_path` relative to the dependency module root
- `ownership: read_only` by default

Missing dependency registry files produce a structured error. Release applications `MUST` treat that as a blocking gate unless a documented bootstrap exception exists.

## 5. CLI

```bash
cargo run -p sdkwork-web-schema-registry -- \
  compose tables \
  --app-root ../sdkwork-clawrouter \
  --registry docs/schema-registry/sdkwork-clawrouter.tables.yaml \
  --output generated/schema/registry/sdkwork-clawrouter.tables.effective.yaml
```

Supported subcommands:

| Subcommand | Purpose |
| --- | --- |
| `compose tables` | Render effective table registry |
| `compose frontend-contracts` | Render compiled frontend field contract |
| `check tables` | Fail when effective output is stale |
| `check frontend-contracts` | Fail when frontend snapshot is stale |

## 6. Python API

```python
from schema_registry.composer import (
    SchemaRegistryComposer,
    load_schema_registry,
    compose_frontend_field_contract,
)
```

Required functions:

- `load_schema_registry(path, app_root=None)`
- `render_schema_registry(path, app_root=None)`
- `schema_registry_source_paths(path, app_root=None)`
- `compose_frontend_field_contract(root, index_path=None)`

## 7. Verification

Framework changes `MUST` pass:

```bash
cargo test -p sdkwork-web-schema-registry
python -m pytest tools/schema_registry/tests -q
```

Application adoption `SHOULD` additionally run the application schema manifest and frontend contract guardians.
