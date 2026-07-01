# Web framework backend-api

Framework control-plane API under `/backend/v3/api/web-framework`.

**Route crate:** `crates/sdkwork-routes-web-framework-backend-api`

**Status:** route handlers and manifest implemented; OpenAPI authority at `openapi.json`.

**Artifacts:**

- `openapi.json` — materialized authority (`cargo test -p sdkwork-routes-web-framework-backend-api materialize_openapi_authority_file -- --ignored`)
- `routes.manifest.json` — route manifest snapshot
