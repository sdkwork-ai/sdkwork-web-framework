# 行业标准对标与封装哲学

> 前置：[00-framework-foundation.md](./00-framework-foundation.md)  
> 本文说明 sdkwork-web-framework **应对齐哪些行业能力**、**如何封装**、**极致标准是什么**。

## 1. 封装哲学（五条）

| # | 原则 | 含义 |
| --- | --- | --- |
| P1 | **Pipeline 先于 Handler** | 横切能力在调用链完成；Handler 零安全样板代码 |
| P2 | **Context 单次解析** | 认证结果进入 `WebRequestContext`；禁止二次解析 Header |
| P3 | **Trait 边界** | 框架定义「何时调用」；业务定义「如何判断」 |
| P4 | **可组合默认** | 每个能力有内存/Noop 默认实现；生产通过装配替换 |
| P5 | **零业务依赖** | 框架 crate 树不指向任何产品仓库 |

对标：**Spring 的「约定优于配置」+ Nest 的「装饰器/管道分层」+ ASP.NET 的「中间件有序管道」+ Stripe 的「幂等/限流一等公民」**，在 Rust 上以 **Type + Trait + Layer** 表达。

## 2. 行业框架能力矩阵

### 2.1 HTTP 管道与横切

| 能力 | Spring Boot | ASP.NET Core | NestJS | Axum/Tower 原生 | sdkwork-web-framework 封装 |
| --- | --- | --- | --- | --- | --- |
| 有序管道 | `FilterChain` | `Middleware` pipeline | Guards→Interceptors→Pipes | `Layer` stack | **`WebCallInterceptorChain`（18 阶段，顺序锁定）** |
| 请求上下文 | `SecurityContextHolder` | `HttpContext` + `ClaimsPrincipal` | `ExecutionContext` | `Extensions` | **`WebRequestContext` + extractor** |
| 认证 | `AuthenticationManager` | `IAuthenticationHandler` | `AuthGuard` | 自定义 `from_fn` | **`WebRequestContextResolver` trait** |
| 授权 | `@PreAuthorize` | Policy / `[Authorize]` | `RolesGuard` | 自定义 | **`AuthorizationPolicy` trait** |
| 异常映射 | `@ControllerAdvice` | `IExceptionHandler` | `ExceptionFilter` | 自定义 | **`WebFrameworkError` → Problem+json** |
| 健康检查 | Actuator `/actuator/health` | `/health` | Terminus | 手写 route | **`service_router` `/healthz` `/readyz`** |
| 指标 | Micrometer | `OpenTelemetry` | Prometheus 模块 | 手写 | **`/metrics` + interceptor 计数** |
| CORS | Spring Security CORS | `UseCors` | `enableCors` | `tower-http::CorsLayer` | **`Cors` interceptor + `CorsPolicy`** |
| 限流 | Bucket4j / Resilience4j | Rate limiter middleware | `@Throttle` | 无标准 | **`RateLimit` interceptor + `RateLimitStore`** |
| 幂等 | 自定义 | 自定义 | 自定义 | 无标准 | **`Idempotency` interceptor + store** |
| 请求 ID | Sleuth / MDC | `TraceIdentifier` | cls-hooked | 手写 | **`RequestIdentity` / `ResponseIdentity`** |
| 契约/OpenAPI | springdoc | Swashbuckle | `@nestjs/swagger` | utoipa | **`HttpRoute` manifest + bootstrap 文档路由** |

### 2.2 SaaS / API 平台

| 能力 | Stripe API | Auth0/Okta | AWS API Gateway | Kong | sdkwork-web-framework |
| --- | --- | --- | --- | --- | --- |
| 版本路径 | `/v1` | — | stage | service route | **`/app|backend|{domain}/v3/api` 标准** |
| API Key | `sk_` + restrict | M2M | usage plan | key-auth plugin | **`ApiKeyLookupService` + open-api 面** |
| OAuth Bearer | Bearer token / session | OIDC / OAuth2 | authorizer | oauth2 plugin | **`OAuthTokenLookupService` + open-api 面** |
| 幂等键 | `Idempotency-Key` | — | — | — | **标准 Header + store** |
| 限流 | 429 + 头 | rate limits | throttle | rate-limiting | **429 + `Retry-After` + Problem+json** |
| 多租户 | Stripe-Account | Organizations | — | — | **`WebRequestPrincipal.tenant_id`** |
| 错误体 | `{error:{type,message}}` | — | — | — | **Problem+json + PlusApiResult（业务层）** |
| 关联 ID | `request-id` | — | `x-amzn-requestid` | — | **`X-Request-Id` 服务端权威** |

### 2.3 多租户 SaaS 后端

| 能力 | Salesforce | Workday | 常见 B2B SaaS | sdkwork-web-framework |
| --- | --- | --- | --- | --- |
| 租户上下文 | OrgId | Tenant | tenant_id in session | **principal.tenant_id** |
| 组织/部门 | — | — | organization_id | **principal.organization_id + login_scope** |
| 数据域 | Sharing rules | — | ABAC tags | **data_scope / permission_scope 标签** |
| 路径不带租户 | ✓ | ✓ | ✓ | **强制：上下文解析，非 path** |
| 部署模式 | Multi-tenant | — | SaaS/单租户 | **deployment_mode: saas/private/local** |

## 3. 封装层次（极致拆分）

```text
┌─────────────────────────────────────────────────────────────────┐
│ L4 业务 Handler / Service（业务仓库）                              │
├─────────────────────────────────────────────────────────────────┤
│ L3 业务 Adapter（Resolver / Authz / Injector — 业务仓库）        │
├─────────────────────────────────────────────────────────────────┤
│ L2 框架装配 WebFrameworkRuntime + with_web_request_context       │
├─────────────────────────────────────────────────────────────────┤
│ L1 框架能力 crate（pipeline / security / context / bootstrap）   │
├─────────────────────────────────────────────────────────────────┤
│ L0 Axum / Tower / HTTP（行业运行时）                             │
└─────────────────────────────────────────────────────────────────┘
```

**极致标准**：业务在 L4 见不到 `Authorization` header 字符串；在 L3 见不到 Axum `Request` 拼装细节；在 L2 一次 `WebFramework::mount(router)` 完成。

## 4. 与「裸 Axum」的差异（为何要框架）

| 裸 Axum 做法 | 问题 | 框架封装 |
| --- | --- | --- |
| 每产品复制 `from_fn` 鉴权 | 漂移、漏洞 | 标准 18 阶段 |
| Handler 内 `headers.get` | 重复、易漏 | `Extension<WebRequestContext>` |
| 各处散落 CORS | 配置不一致 | `SecurityPolicy` + 阶段 |
| 无 operationId 关联 | 观测碎片化 | manifest + 日志字段标准 |
| 租户 ID 从 query 取 | 越权 | `TenantIsolationPolicy` |

## 5. 极致打磨目标（North Star）

| 维度 | 目标 |
| --- | --- |
| **安全默认** | deny-by-default CORS；无 token 即 401；伪造头无效 |
| **可测性** | 全链可用 `TestRuntime` 内存装配，无需 HTTP server |
| **可观测** | 每阶段 span；metrics 按 surface/operation 聚合 |
| **可扩展** | 18 阶段可插自定义 interceptor，不可删 mandatory |
| **可迁移** | Java/Rust 同 operationId 同上下文词汇 |
| **性能** | 热路径零分配原则（request_id 除外）；store 异步非阻塞 |
| **文档即契约** | 每个能力在 capability catalog 有 M 等级与验收项 |

## 6. 明确不 reinvent

| 不自己做 | 用行业成熟件 |
| --- | --- |
| HTTP 协议解析 | hyper / axum |
| TLS | rustls（业务/部署层） |
| SQL 驱动 | sqlx（仅 store crate） |
| JSON | serde_json |
| 分布式 trace 导出 | OpenTelemetry（可选 feature，observability crate） |

## 7. 相关文档

- 全量能力目录：[13-capability-catalog.md](./13-capability-catalog.md)
- 标准体系：[14-standards-system.md](./14-standards-system.md)
- 扩展点注册表：[15-extension-points-registry.md](./15-extension-points-registry.md)
- 成熟度模型：[16-maturity-model.md](./16-maturity-model.md)
