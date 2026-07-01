# 架构设计

> 前置阅读：[00-framework-foundation.md](./00-framework-foundation.md)

## 1. 架构目标

构建 **零业务依赖** 的 SDKWork Web/SaaS 基础框架：

- 对 Axum/Tower **集成封装**，而非重写 HTTP 栈
- **制定并强制执行** SaaS API 横切标准
- **封装常用能力**（上下文、安全链、服务基座、契约类型）
- 业务通过 **trait 扩展** 注入认证、授权、领域上下文

## 2. 分层模型

```text
┌──────────────────────────────────────────────────────────────────┐
│ L3  Application Roots（apps/、Tauri host、gateway 装配）            │
├──────────────────────────────────────────────────────────────────┤
│ L2  Business Capability Repos（appbase, claw-router, commerce…） │
│     • sdkwork-routes-<capability>-<surface>  （路由+Handler）     │
│     • *-service / *-repository               （业务逻辑）          │
│     • *-web-adapter                          （实现框架 trait）    │
├──────────────────────────────────────────────────────────────────┤
│ L1  sdkwork-web-framework  ◄── 本仓库                           │
│     contract → context → pipeline → security → axum → bootstrap  │
├──────────────────────────────────────────────────────────────────┤
│ L0  sdkwork-specs + Axum/Tower/Hyper                             │
└──────────────────────────────────────────────────────────────────┘
```

**依赖规则**：仅允许 L2→L1、L3→L2/L1；L1 不得依赖 L2/L3。

## 3. 框架内 Crate 架构

```text
sdkwork-web-framework/crates/

  sdkwork-web-contract        # 纯类型：ApiSurface, HttpRoute, RouteAuth
        ▲
  sdkwork-web-core            # WebRequestContext, Interceptor chain, SecurityPolicy, stores
        ▲
  sdkwork-web-axum            # Layer, middleware, Extension extractors
        ▲
  sdkwork-web-bootstrap       # service_router, healthz, metrics, openapi routes

  sdkwork-web-store-sqlx      # 可选 feature：web_* 表 sqlx 适配（无业务 SQL）
  sdkwork-web-store-redis     # 可选 feature：流控 Redis 适配
```

### 3.1 聚合入口

业务可只依赖一个 facade crate（可选后续 `sdkwork-web`）：

```rust
pub use sdkwork_web_bootstrap::*;
pub use sdkwork_web_axum::{with_web_request_context, WebFrameworkRuntime};
pub use sdkwork_web_core::{WebRequestContext, WebRequestPrincipal};
```

### 3.2 各 Crate 禁止事项

| Crate | 禁止 |
| --- | --- |
| 全部 | 依赖任何 `sdkwork-appbase`、`sdkwork-iam-*`、产品 router |
| `sdkwork-web-contract` | 依赖 axum、tokio |
| `sdkwork-web-core` | 具体验签、数据库、IAM 类型 |
| `sdkwork-web-axum` | 业务 Handler、SQL |
| `sdkwork-web-bootstrap` | 业务 contract manifest 内容 |
| `sdkwork-web-store-*` | 访问 `iam_*` 或产品表 |

## 4. 运行时组装（业务侧）

框架提供 **`WebFrameworkRuntime`** 装配对象：

```rust
pub struct WebFrameworkRuntime<R, A> {
    pub resolver: R,                              // 业务实现
    pub profile: WebRequestContextProfile,
    pub security_policy: SecurityPolicy,
    pub authorization: Option<Arc<dyn AuthorizationPolicy>>,
    pub tenant_isolation: Option<Arc<dyn TenantIsolationPolicy>>,
    pub domain_injectors: Vec<Arc<dyn DomainContextInjector>>,
    pub rate_limit_store: Arc<dyn RateLimitStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub audit_emitter: Arc<dyn AuditEmitter>,
}
```

业务启动（appbase 示例）：

```text
IamWebRequestContextResolver
IamAuthorizationPolicy
IamDomainContextInjector  → WebFrameworkRuntime → with_web_request_context
                                                      → sdkwork-routes-iam-app-api
```

## 5. 请求路径（框架视角）

```text
HTTP Request
  → sdkwork-web-axum::with_web_request_context
       → sdkwork-web-core::WebCallInterceptorChain (18 stages)
            → 调用 sdkwork-web-core policies & stores (trait)
            → 调用业务 resolver/authorization (trait object)
       → Extension<WebRequestContext>
       → 业务 Handler（在业务 router crate）
  → HTTP Response
```

框架 **止于** Handler 边界之前；Handler 及之后属业务仓库。

## 6. 三接口面（标准，非业务路由）

框架定义 **分类标准**，不注册业务路径：

| Surface | 默认前缀 | AuthMode |
| --- | --- | --- |
| `AppApi` | `/app/v3/api` | `DualToken` |
| `BackendApi` | `/backend/v3/api` | `DualToken` |
| `OpenApi` | 可配置多前缀 | `ApiKey` |
| `Public` | `public_path_prefixes` | `Public` |

业务 router 挂载在对应前缀下；分类由框架 `SurfaceClassification` 自动完成。

## 7. SaaS 多租户（框架类型层）

框架定义 **principal 词汇**（`WebRequestPrincipal`），不定义租户数据如何存储：

```text
tenant_id → organization_id → app_id → environment → deployment_mode
         → data_scope / permission_scope（字符串标签，业务解释语义）
```

租户隔离 **接口** `TenantIsolationPolicy::enforce(ctx, operation_id)` 在框架；**规则** 在业务。

## 8. 双 Pipeline

| Pipeline | 所属 | 说明 |
| --- | --- | --- |
| HTTP `WebCallInterceptorChain` | **框架** | 每个请求必经 |
| Domain `InvocationPipeline` | **业务**（如 claw-product） | 可选；Gateway 计费等 |

业务 Domain Pipeline **不得**替代 HTTP 链的认证与租户隔离。

## 9. 与 sdkwork-clawrouter 关系

| 项 | 关系 |
| --- | --- |
| 依赖方向 | claw-router **依赖** web-framework |
| `sdkwork-claw-http` | 逐步改为对 `sdkwork-web-bootstrap` 的薄包装或 re-export |
| `InvocationPipeline` | 留在 claw-product，与框架 HTTP 链正交 |
| 重复 auth middleware | 收敛到框架 Interceptor + 业务 Resolver |

## 10. 与 sdkwork-appbase 关系

| 项 | 关系 |
| --- | --- |
| 依赖方向 | appbase **依赖** web-framework |
| IAM 路由 | 留在 appbase `sdkwork-routes-iam-*` |
| IAM 验签 / API Key / OAuth Bearer 查表 | appbase `sdkwork-iam-web-adapter` 实现框架 trait |
| `IamAppContext` | 仅 appbase；经 `DomainContextInjector` 注入 |

## 11. 标准强制执行点

| 标准 | 模块 |
| --- | --- |
| API_SPEC §10.2 链顺序 | `sdkwork-web-core` |
| SECURITY_SPEC §5.1 挂载点 | `sdkwork-web-core` pipeline |
| WEB_BACKEND_SPEC Handler 薄层 | 文档 + `WebRequestContext` extractor |
| Problem+json | `sdkwork-web-core::WebFrameworkError` |

## 12. 质量门禁

```bash
# 框架仓库内
cargo test --workspace
cargo tree --workspace | verify no business crates
```

- 契约测试：18 阶段顺序
- 架构测试：`tests/architecture/no_business_dependencies.rs`
- 安全测试：CORS deny、伪造身份头
