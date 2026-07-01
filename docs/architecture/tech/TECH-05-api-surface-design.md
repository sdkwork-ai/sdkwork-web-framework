> Migrated from `docs/architecture/tech/TECH-05-api-surface-design.md` on 2026-06-24.
> Owner: SDKWork maintainers

# API 接口面设计

> 前置阅读：[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)

## 1. 框架职责边界

| 框架定义 | 框架不定义 |
| --- | --- |
| 三接口面 **语义**与前缀 **标准** | 具体业务 path/operationId |
| `ApiSurface`、`HttpRoute`、`RouteAuth` 类型 | 任何 `sdkwork-routes-<业务>-*` |
| `contract_fallback` **行为**（501/404 规则） | 产品 OpenAPI authority 内容 |
| `service_router` 基础设施路径 | IAM/电商/网关业务 Handler |

**所有业务 API 清单归属各业务仓库**；框架只提供 **挂载与分类标准**。

## 2. 三接口面标准

| 接口面 | 前缀 | 认证 | 响应信封（SDKWork CRUD 面） |
| --- | --- | --- | --- |
| app-api | `/app/v3/api` | Dual Token | `SdkWorkApiResponse<T>`（`API_SPEC.md` §15） |
| backend-api | `/backend/v3/api` | Dual Token | `SdkWorkApiResponse<T>` |
| open-api | `/{domain}/v3/api` 等 | API Key / OAuth Bearer / `OpenApiFlexible` | `SdkWorkApiResponse<T>` |
| gateway-api | `/v1/**` 等 | 产品定义 | 协议原生（非 SDKWork 包装） |

## 3. 业务路由所有权（WEB_BACKEND_SPEC）

```text
<业务仓库>/crates/sdkwork-routes-<capability>-<surface>/
  paths.rs / routes.rs / handlers.rs / manifest.rs
```

示例：

| 仓库 | Route crate |
| --- | --- |
| sdkwork-appbase | `sdkwork-routes-iam-app-api` |
| sdkwork-clawrouter | `sdkwork-routes-clawrouter-app-api`（产品域） |
| sdkwork-shop | `sdkwork-routes-shop-app-api` |

## 4. 框架基础设施路径（sdkwork-web-bootstrap）

| Path | 说明 |
| --- | --- |
| `/healthz` | 存活 |
| `/readyz` | 就绪（可注入 DB check） |
| `/metrics` | Prometheus 文本 |
| `/openapi/app.json` | 可选：挂载业务 manifest 聚合 |
| `/openapi/backend.json` | 同上 |

OpenAPI 文档路由 **装配**业务提供的 manifest，框架不包含业务 operation。

## 5. HttpRoute 契约类型（sdkwork-web-contract）

```rust
pub struct HttpRoute {
    pub method: HttpMethod,
    pub path: &'static str,
    pub tag: &'static str,
    pub operation_id: &'static str,
    pub auth: RouteAuth,
    pub idempotent: bool,
    pub rate_limit_tier: Option<RateLimitTier>,
}
```

业务 `manifest.rs` 使用此类型；物化脚本在 **业务仓库** `sdks/` 执行。

## 6. contract_fallback 标准

- 契约 manifest 有、Handler 无 → `501` + 结构化 envelope
- 契约 manifest 无 → `404`
- manifest 由 **业务** 传入 `ServiceRouterConfig::with_contract_fallback_from_manifest` 或 `WebFrameworkBuilder::route_manifest`（自动挂载 fallback）

## 7. OpenAPI 扩展（框架推荐）

```yaml
x-sdkwork-request-context: WebRequestContext
x-sdkwork-api-surface: app-api
```

## 8. 框架 control-plane backend-api（显式例外）

本仓库 **保留** 框架自有 control-plane 路由 crate，与 `WEB_FRAMEWORK_STANDARD.md` §8 一致：

| 组件 | 说明 |
| --- | --- |
| `sdkwork-routes-web-framework-backend-api` | `/backend/v3/api/web-framework/*` 治理 API |
| `apis/backend-api/web-framework/` | OpenAPI authority + routes manifest |
| `apps/sdkwork-web-framework-pc` | 可选 PC 运维控制台（UI → hook → service → SDK） |

跨产品 Web 运行时治理 **可以** 在本仓库通过上述 control-plane 面完成；业务产品 **不得** 新增 `sdkwork-routes-<业务>-*` crate 到本仓库。

## 9. 错误契约

- 拦截器 / extractor / fallback / handler 错误：`application/problem+json`（含 numeric `code` 与 `traceId`）
- 业务 Handler 成功体：`SdkWorkApiResponse<T>`（`sdkwork-specs/API_SPEC.md` §15）

## 10. 验收

- [x] 无 **业务** `sdkwork-routes-*` crate（允许 `sdkwork-routes-web-framework-backend-api` control-plane 例外）
- [x] 框架 manifest 仅含 `webFramework.*` operationId（`routes_contract` / `openapi_authority`）
- [ ] appbase/claw-router 各自 manifest 使用 `HttpRoute` 类型（消费者仓库验收）

