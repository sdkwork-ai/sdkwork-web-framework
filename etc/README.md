# SDKWork Web Framework Source Configuration

Entrypoint: `etc/sdkwork.deployment.config.json`.

- `topology/` owns the resolved per-profile env files.
- `configs/` owns safe, non-secret runtime config templates.
- `client-env.materialization.json` owns client surface materialization.

Validate:

```bash
node ../../sdkwork-specs/tools/check-source-config-standard.mjs --root .
node ../../sdkwork-specs/tools/materialize-topology-env.mjs --root . --check
```
