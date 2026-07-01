> Migrated from `docs/architecture/tech/TECH-13-capability-catalog.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 能力目录（功能点 + 技术点）

> 前置：[TECH-12-industry-framework-benchmark.md](./TECH-12-industry-framework-benchmark.md)  
> 成熟度等级见 [TECH-16-maturity-model.md](./TECH-16-maturity-model.md)（M0–M4）。

## 1. 能力域总览

```text
A. 请求生命周期    B. SaaS 多租户    C. 安全           D. 韧性
E. 可观测性        F. 契约治理       G. 错误与响应     H. 运行时引导
I. 存储适配        J. 验证与绑定     K. 测试与质量     L. 部署剖面
```

---

## A. 请求生命周期

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| A1 | 服务端 Request ID | UUID v4；覆盖客户端 `X-Request-Id` | pipeline: RequestIdentity | M3 |
| A2 | API 面分类 | 前缀匹配；profile 可配置多 open 前缀 | pipeline: SurfaceClassification | M3 |
| A3 | 公共路径免认证 | manifest `RouteAuth::Public` + infra `public_path_prefixes`；`resolve_public_path` | Surface + Authentication | M3 |
| A4 | 凭证提取 | Bearer / Access-Token / X-API-Key / OAuth Bearer 规范化 | context: extractors + open_api_auth | M3 |
| A5 | 上下文单次解析 | Resolver async；结果不可变 | pipeline: RequestContextResolution | M3 |
| A6 | 上下文注入 | `Extension<WebRequestContext>` | pipeline: ContextInjection | M3 |
| A7 | 响应 Request ID 回写 | 所有成功/错误响应 | pipeline: ResponseIdentity | M3 |
| A8 | 领域上下文注入 | `DomainContextInjector` 列表 | pipeline: ContextInjection | M2 |
| A9 | Operation 绑定 | 从 manifest 或 route 属性解析 `operation_id` | contract + logging | M2 |
| A10 | 请求超时 | `with_request_timeout` + `WebFrameworkBuilder::request_timeout`；生产默认 30s | axum + bootstrap | M3 |
| A11 | 请求体缓冲策略 | 流式 vs 限制大小 | security: RequestSizeLimit | M3 |
| A12 | HTTP 方法约束 | OPTIONS/GET/POST/… 白名单 | pipeline: MethodGuard | M3 |
| A13 | Open-api API Key 解析 | `ApiKeyLookupService` + `resolve_api_key` | resolvers + open_api_auth | M3 |
| A14 | Open-api OAuth Bearer 解析 | `OAuthTokenLookupService` + `resolve_oauth_bearer` | resolvers + open_api_auth | M3 |
| A15 | Open-api 多凭证选择 | `OpenApiCredentialSchemeDetector` + `RouteAuth::OpenApiFlexible` | open_api_auth | M3 |

---

## B. SaaS 多租户

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| B1 | Principal 词汇 | tenant/org/user/session/app/env/deployment | context: WebRequestPrincipal | M3 |
| B2 | Login Scope | tenant vs organization 与 org_id 一致性强校验 | resolver 契约 + TenantIsolation | M3 |
| B3 | Data Scope 标签 | `Vec<String>` 不透明标签，业务解释 | principal 字段 | M3 |
| B4 | Permission Scope | operation grant 标签 | principal + AuthorizationPolicy | M2 |
| B5 | 租户隔离 enforcement | `TenantIsolationPolicy::enforce` | pipeline: TenantIsolation | M2 |
| B6 | 平台级上下文 | `tenant_id = "0"` 词汇（平台共享） | 文档 + policy | M1 |
| B7 | Workspace / Composition | 组合运行时隔离字段 | principal 可选字段 | M2 |
| B8 | 禁止路径租户 | 分类器不识别 `/tenants/{id}` 为合法 SaaS 路径 | contract  lint | M2 |
| B9 | 禁止客户端身份头 | 忽略 `x-sdkwork-tenant-id` 等列表 | resolver 基类 + 测试 | M3 |
| B10 | 部署剖面 | saas / private / local 影响 dev resolver | context: DeploymentMode | M3 |

---

## C. 安全

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| C1 | CORS 显式白名单 | deny-by-default；credentials 规则 | security + Cors stage | M3 |
| C2 | 跨站状态变更防护 | Origin/Referer 校验 | CrossSiteRequest | M3 |
| C3 | CSRF（Cookie 流） | 可选 `CsrfPolicy` interceptor | security: optional | M1 |
| C4 | SQL 注入启发式 | 可配置 header 探测 | SqlInjectionGuard | M3 |
| C5 | 安全响应头 | nosniff, frame, referrer, permissions | HeaderSecurity | M3 |
| C6 | HSTS | HTTPS 环境可选 | HeaderSecurity config | M2 |
| C7 | 内容类型嗅探防护 | JSON API Content-Type 校验 | 可选 stage | M1 |
| C8 | Open-api 凭证不明文日志 | API Key / OAuth Bearer redact 规则 | security + observability | M3 |
| C9 | 双 Token 冲突拒绝 | resolver 契约测试 | 业务 adapter + 框架测试向量 | M3 |
| C10 | Auth 模式匹配 | surface ↔ DualToken/ApiKey/OAuth/OpenApiFlexible/Public | Authentication stage | M3 |
| C11 | 授权决策点 | `AuthorizationPolicy` | Authorization stage | M2 |
| C12 | 安全事件发射 | rate_limit_exceeded, cors_denied | security event emitter | M2 |
| C13 | 动态 CORS 策略 | `DynamicCorsPolicySource` + Sqlx 实现 | cors_policy + EP-16 | M2 |
| C14 | JWT 租户绑定验签 | HS256/RS256 + `kid` + claim 绑定 + exp/token_type + EP-05e 吊销 | jwt_tenant + resolvers | M3 |

---

## D. 韧性（限流 / 幂等 / 重试边界）

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| D1 | 分布式限流 | 滑动窗口/令牌桶；Redis | RateLimitStore redis | M3 |
| D2 | 本地限流 | moka 内存 | RateLimitStore memory | M3 |
| D3 | 限流键哈希 | 无 PII/token 明文 | key builder util | M3 |
| D4 | 429 响应 | Retry-After + Problem+json | RateLimit interceptor | M3 |
| D5 | 幂等键 | Idempotency-Key / X-Idempotency-Key | Idempotency interceptor | M3 |
| D6 | 幂等体哈希 | 相同 key 不同 body → 409 | idempotency fingerprint | M3 |
| D7 | 幂等响应重放 | 缓存 status + body | IdempotencyStore | M3 |
| D8 | 限流 tier | auth_critical / open_api_default | HttpRoute.rate_limit_tier | M2 |
| D9 | 并发限制 | 每 tenant 连接数 | ConcurrentAdmissionStore + Redis | M2 |
| D10 | 熔断 | 下游失败率（业务/domain） | **不在 HTTP 框架** | — |

---

## E. 可观测性

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| E1 | 结构化日志 | tracing span 每请求 | pipeline: Logging | M3 |
| E2 | 阶段耗时 | 每 interceptor before 计时 | pipeline metrics hook | M2 |
| E3 | Prometheus 指标 | `http_requests_total` 等 | bootstrap: /metrics | M3 |
| E4 | 指标标签 | surface, method, status, operation | metrics labels 标准 | M2 |
| E5 | 日志脱敏 | token/apikey/password 过滤 | observability: RedactingLayer | M3 |
| E6 | 路由模板日志 | 记 template 非 raw path | OBSERVABILITY_SPEC 对齐 | M2 |
| E7 | 审计关联 | audit 含 request_id | AuditEmitter | M2 |
| E8 | Trace 传播 | traceparent 解析/生成 | optional otel feature | M1 |
| E9 | 健康检查 | liveness vs readiness | bootstrap | M3 |
| E10 | 就绪依赖检查 | 注入 `ReadinessCheck` fn | bootstrap | M3 |

---

## F. 契约治理

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| F1 | 三接口面枚举 | AppApi/BackendApi/OpenApi | contract: ApiSurface | M3 |
| F2 | HttpRoute 清单类型 | method/path/operationId/auth | contract | M3 |
| F3 | contract_fallback | 501/404 Problem+json + `traceId`；`service_router` fallback + `WebFrameworkBuilder::route_manifest` | bootstrap | M3 |
| F4 | OpenAPI 文档路由 | 挂载业务 manifest 聚合 | bootstrap | M2 |
| F5 | operationId 稳定 | manifest lint | 业务 + 共享 lint 规则 | M2 |
| F6 | x-sdkwork-* 扩展 | request-context, api-surface | 文档标准 | M2 |
| F7 | 响应信封区分 | Problem vs SdkWorkApiResponse | error mapping 文档 | M3 |
| F8 | Gateway 面独立 | `/v1` 不强制 Plus 包装 | profile.gateway_prefixes | M2 |

---

## G. 错误与响应

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| G1 | Problem+json + SdkWorkApiResponse | RFC 9457 `ProblemDetail` with numeric `code` + `traceId`; success `{ code: 0, data, traceId }`; `finish_api_json` / `WebFrameworkRejection` | context: WebFrameworkError | M3 |
| G2 | 错误 type URI | `https://sdkwork.dev/problems/*` | error catalog | M2 |
| G3 | 阶段错误映射 | 401/403/429/413/405 | interceptor → status | M3 |
| G4 | 验证错误 | 业务 DTO；框架不绑 validator | 业务层 serde/validator | — |
| G5 | 国际化错误 | message 可选；code 稳定 | Problem detail | M1 |
| G6 | 错误不泄露栈 | prod 无 backtrace 到客户端；禁止 bare `IntoResponse` 默认关联 | problem_correlation_rules | M3 |

---

## H. 运行时引导（Bootstrap）

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| H1 | service_router 基座 | health/metrics/openapi | bootstrap | M3 |
| H2 | WebFramework::builder | 流式装配 resolver/store/policy | axum: WebFrameworkRuntime | M3 |
| H3 | Router merge 约定 | 业务子 router 注入 state | 文档 + 示例 | M2 |
| H4 | 优雅关闭 | SIGTERM/Ctrl+C | bootstrap: serve helper | M2 |
| H5 | 环境配置 | `SDKWORK_WEB_*` env 词汇 | config module | M2 |
| H6 | Feature flags | cargo features: redis/sqlx/otel | workspace | M2 |
| H7 | 单机嵌入 | Tauri/单进程 merge | 文档 | M2 |
| H8 | 多服务组合 | 与 api-gateway 嵌入点 | 文档 | M1 |

---

## I. 存储适配（仅 web_*）

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| I1 | RateLimitStore trait | async incr/check | security | M3 |
| I2 | IdempotencyStore trait | get/put + TTL | security | M3 |
| I3 | AuditEmitter trait | append-only | security | M2 |
| I4 | SecurityEventEmitter | 安全告警 | security | M2 |
| I5 | Sqlx 实现 | web_* migrations | store-sqlx | M2 |
| I6 | Redis 实现 | 热路径限流 | store-redis | M2 |
| I7 | Memory 实现 | 测试/dev 默认 | security | M3 |

---

## J. 验证与绑定（框架边界）

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| J1 | Typed extractor | `WebRequestContext`, `RequirePrincipal` | axum | M3 |
| J2 | Auth mode guard | extractor `RequireDualToken` | axum | M2 |
| J3 | Surface guard | `RequireAppApi` | axum | M2 |
| J4 | 请求体大小预检 | Content-Length | RequestSizeLimit | M3 |
| J5 | JSON 大小与框架协作 | 超限 413 | 同上 | M3 |

---

## K. 测试与质量

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| K1 | 链顺序契约测试 | 18 stage 不可变 | tests/contract | M3 |
| K2 | TestRuntime | 内存 resolver/store | test-utils crate | M3 |
| K3 | 架构测试 | 无业务依赖 cargo tree | tests/architecture | M3 |
| K4 | 安全向量测试 | 伪造头/CORS/限流 | tests/security | M3 |
| K5 | 金标 Problem JSON | snapshot | tests/snapshot | M2 |
| K6 | 并发限流测试 | tokio 压测 | tests/stress | M2 |
| K7 | Java parity checklist | 同阶段名/字段 | 文档 + 共享 vectors | M1 |
| K8 | SQLx migration guard | `web_*` 迁移不可变 | tests/sqlx_migrations | M2 |
| K9 | Release pipeline benchmark | p99 < 0.5ms @ release | `pipeline_benchmark` + `scripts/benchmark-pipeline.*` | M4 |
| K10 | Problem 关联架构守卫 | 禁止 bare IntoResponse / about:blank | problem_correlation_rules | M3 |
| K11 | Contract fallback 架构守卫 | manifest → fallback 静态校验 | bootstrap_contract_fallback | M3 |
| K12 | 商业化 GA 就绪守卫 | CHANGELOG/runbook/env/verify 对齐 | commercial_ga_readiness | M3 |
| K13 | WebSocket 安全向量 | 未认证连接/超大消息/速率限制 | ws_security_vectors | M3 |
| K14 | Manifest 驱动 PC Admin SDK | operations.ts 由 manifest 生成 | generate-pc-admin-operations.mjs | M3 |
| K15 | PC Admin 构建冒烟 | dist bundle 保留控制台壳层与分层 | pc-admin-build.smoke.test.mjs | M3 |
| K16 | PC Admin Playwright E2E | 预览壳层加载、权限 Tab 与 mock backend | e2e/console.smoke.spec.ts | M3 |
| K17 | PC Admin 真实 backend E2E | assemble_control_plane + dual-token SDK | e2e/console.integration.spec.ts | M3 |
| K18 | 生产 Rollout / 多产品采纳证据 | Pre-flight→Canary→Rollback + adoption JSON | 24-production-rollout-and-adoption.md | M4 |
| K19 | Release 证据包 | git SHA + verify 命令 + rollout/adoption 路径 | collect-release-evidence.mjs | M4 |

---

## W. WebSocket

| ID | 功能点 | 技术点 | 框架模块 | 目标 M |
| --- | --- | --- | --- | --- |
| W1 | HTTP 管道共享 | upgrade 前运行标准 HTTP 拦截链 | axum + with_web_request_context | M3 |
| W2 | WS 标准拦截器 | connect/message/close 链 | WebSocketCallInterceptorChain | M2 |
| W3 | 消息大小限制 | `PayloadTooLarge` / 413 语义 | ws_interceptors:MessageSizeLimit | M3 |
| W4 | 消息速率限制 | 租户/会话窗口计数 | ws_interceptors:MessageRateLimit | M3 |
| W5 | 连接需 Principal | 未认证 WS 连接拒绝 | ws_interceptors:PrincipalRequired | M3 |

---

## L. 部署剖面（框架行为差异）

| 剖面 | 框架行为 |
| --- | --- |
| **saas** | Redis 限流；严格 CORS；`tenant_bound_verifying_web_request_resolver`；禁止 dev resolver |
| **private** | 可 PG 限流；单租户默认 profile |
| **local/dev** | 内存 store；可选 claim-string resolver；宽松 CORS profile |
| **test** | `TestRuntime`；固定 principal fixture |

---

## 2. 能力统计（目标态）

| 等级 | 数量（约） | 说明 |
| --- | --- | --- |
| M3+ 必须 GA | 45+ | 核心 SaaS HTTP 路径 |
| M2 GA+ | 25+ | 运维/观测/可选安全 |
| M1 规划 | 10+ | OTel/CSRF 等 |
| M0/业务 | 熔断等业务韧性 | 不在框架 |

## 3. 相关文档

- [TECH-14-standards-system.md](./TECH-14-standards-system.md)
- [TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md)
- [specs/web-framework-capability.matrix.json](../specs/web-framework-capability.matrix.json)

