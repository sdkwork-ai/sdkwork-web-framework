> Migrated from `docs/architecture/tech/TECH-00-framework-foundation.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 基础框架定位与依赖法则

## 1. 一句话定义

**`sdkwork-web-framework` 是所有带 HTTP API 的 SDKWork 能力仓库所依赖的 Web/SaaS 基础底层框架**——它对 Axum/Tower 等运行时做集成封装，制定 Web 开发与 SaaS 多租户开发标准，并提供可插拔的通用能力；**它不实现任何业务域，也不依赖任何业务仓库**。

## 2. 生态位

```text
                    sdkwork-specs
                 （全局契约与命名标准）
                          │
                          ▼
              sdkwork-web-framework          ◄── 本仓库：纯框架层
         （Web 集成封装 + SaaS 标准 + 通用能力）
                          │
        ┌─────────────────┼─────────────────┬─────────────────┐
        ▼                 ▼                 ▼                 ▼
 sdkwork-appbase   sdkwork-clawrouter  sdkwork-shop   sdkwork-aiot
 （IAM 等业务）      （AI 网关等业务）     （电商等业务）      （物联网等业务）
        │                 │                 │                 │
        └─────────────────┴─────────────────┴─────────────────┘
                          │
                    各产品 route crate
              sdkwork-routes-<capability>-<surface>
```

### 2.1 依赖方向（铁律）

| 方向 | 是否允许 | 说明 |
| --- | --- | --- |
| 业务仓库 → `sdkwork-web-framework` | ✅ **必须** | appbase、claw-router、shop 等 T1 仓库集成框架 |
| `sdkwork-web-framework` → 业务仓库 | ❌ **禁止** | 不得依赖 appbase、iam、claw-router、commerce 等业务 crate |
| `sdkwork-web-framework` → `sdkwork-specs` | ✅ 文档引用 | 规范通过相对路径引用，不 cargo 依赖 |
| `sdkwork-web-framework` → 通用基础设施 | ✅ 受限 | 见 §4 白名单 |

**`sdkwork-appbase` 依赖 `sdkwork-web-framework`，而不是反过来。**

## 3. 框架 vs 业务：所有权边界

| 归属 | 内容 |
| --- | --- |
| **框架独有** | `WebRequestContext`、Interceptor 链、SecurityPolicy、Resolver **trait**、Axum 中间件封装、`service_router` 基座、契约类型 `HttpRoute`/`ApiSurface`、Problem+json、框架 `web_*` 运行时表契约、内存/可插拔 Store 默认实现 |
| **业务独有** | IAM 用户/会话/API Key 表、`IamAppContext`、具体 `WebRequestContextResolver` 实现、`AuthorizationPolicy` 业务规则、所有 `sdkwork-routes-*` 路由与 Handler、OpenAPI authority、产品 SDK 家族 |
| **框架定义标准、业务实现扩展** | 双 Token 解析流程、API Key / OAuth Bearer 查找接口、open-api 凭证 scheme 检测、租户隔离校验接口、审计/流控 **语义与挂载点** |

## 4. 技术依赖白名单

框架 crate **仅允许**依赖以下类别（不得引入业务 workspace member）：

| 类别 | 允许 crate | 用途 |
| --- | --- | --- |
| Web 运行时 | `axum`, `tower`, `tower-http`, `http`, `http-body` | HTTP 集成封装 |
| 异步 | `tokio`, `async-trait`, `futures` | 异步 Interceptor |
| 序列化 | `serde`, `serde_json` | 上下文与错误 |
| 观测 | `tracing` | 结构化日志（可选 feature） |
| 随机/ID | `getrandom`, `uuid` | request_id |
| 缓存/存储适配 | `redis`, `sqlx`, `sdkwork-database-config`, `sdkwork-database-sqlx` | **仅** `sdkwork-web-store-*` 可选 crate；连接池经 `sdkwork-database-sqlx` 创建，store 实现只访问 `web_*` 表 |
| 错误 | `thiserror` | 库边界 |

**禁止依赖（示例）**：`sdkwork_iam_context_service`、`sdkwork-claw-*`、`sdkwork-commerce (deleted)-*-service`、任何 `sdkwork-routes-*`。

## 5. 框架提供的「封装抽象」

`sdkwork-web-framework` 不是「又一个产品服务」，而是 **Web 框架集成层**：

```text
┌─────────────────────────────────────────────────────────────┐
│  Application（业务仓库）                                       │
│  sdkwork-routes-iam-app-api / sdkwork-routes-merchandise-app-api │
├─────────────────────────────────────────────────────────────┤
│  sdkwork-web-bootstrap     service_router / health / metrics │
│  sdkwork-web-axum          with_web_request_context Layer    │
├─────────────────────────────────────────────────────────────┤
│  sdkwork-web-core          WebRequestContext, InterceptorChain, SecurityPolicy │
│  sdkwork-web-contract      ApiSurface / HttpRoute            │
├─────────────────────────────────────────────────────────────┤
│  Axum 0.8 + Tower 0.5 + Hyper（行业 Web 栈，技术选型见 doc 11）│
└─────────────────────────────────────────────────────────────┘
```

业务代码 **只接触框架公开 API**，不直接拼装安全链、不重复实现 CORS/流控挂载点。

## 6. SaaS Web 开发标准（框架权威）

本仓库与 `sdkwork-specs/API_SPEC.md` §10、`WEB_BACKEND_SPEC.md`、`SECURITY_SPEC.md` 共同构成可执行标准；框架 **强制执行** 的部分包括：

| 标准项 | 框架 enforcement |
| --- | --- |
| 三接口面前缀 | `WebRequestContextProfile` 分类 |
| 请求上下文单次解析 | `RequestContextResolution` 阶段 |
| 禁止客户端身份头 | 解析器忽略 + 文档化 |
| 18 阶段 Interceptor 顺序 | `WebCallInterceptorChain::standard()` |
| Problem+json 错误 | `WebFrameworkError` |
| Handler 消费 typed context | `Extension<WebRequestContext>` extractor |
| 多租户 principal 词汇 | `WebRequestPrincipal` 字段 |
| 安全响应头 | `HeaderSecurity` 阶段 |
| 流控/幂等挂载点 | trait + 可选 store，业务配置策略 |

业务标准（框架只提供扩展点，不实现）：

| 标准项 | 业务实现位置 |
| --- | --- |
| IAM 双 Token 验签 | appbase `IamWebRequestContextResolver` |
| RBAC/permission_scope | appbase `IamAuthorizationPolicy` |
| API Key 记录查询 | appbase `IamApiKeyLookupService` |
| OAuth Bearer 记录查询 | appbase `IamOAuthTokenLookupService` |
| Open-api 多凭证解析 | appbase `IamOpenApiWebRequestContextResolver` + `OpenApiCredentialSchemeDetector` |
| 产品 OpenAPI / SDK | 各产品 `sdks/` |

## 7. 领域上下文注入（通用机制）

框架 **不知道** `IamAppContext`、订单上下文等任何领域类型。

提供通用扩展点：

```rust
/// 业务在装配时注册，将 WebRequestContext 映射为领域上下文并写入 Extensions
pub trait DomainContextInjector: Send + Sync {
    fn inject(&self, request: &mut Request, ctx: &WebRequestContext);
}
```

- appbase 注册 `IamDomainContextInjector`（内部 `WebRequestPrincipal` → `IamAppContext`）
- T1 commerce 仓库（shop、order、payment 等）可注册自己的 injector
- 框架 `ContextInjection` 阶段只调用已注册的 `Vec<Arc<dyn DomainContextInjector>>`

## 8. 仓库内容边界

### 8.1 本仓库 **包含**

- `crates/sdkwork-web-*` 框架 crate
- `specs/` 框架 component 契约
- `docs/` 框架设计与标准说明
- `tests/` 框架契约测试（无业务 fixture）
- 可选：`sdks/sdkwork-web-framework-types-sdk`（纯类型/bootstrap，无业务 API）

### 8.2 本仓库 **不包含**

- 任何 `sdkwork-routes-<业务>-*` 路由 crate
- IAM / 电商 / 网关等业务 Handler
- 业务数据库 migration（`iam_*` 等）
- 业务产品前端（`apps/` 仅保留 **框架自有** PC 管理台 demo：`sdkwork-web-framework-pc`，见 H8）
- 对产品 OpenAPI authority 的物化（除框架 types/bootstrap）

## 9. 消费者集成清单（以 appbase 为例）

```text
sdkwork-appbase/Cargo.toml:
  sdkwork-web-core = { path = "../sdkwork-web-framework/crates/sdkwork-web-core" }
  sdkwork-web-axum = { path = "..." }

sdkwork-appbase 新增/调整:
  sdkwork-iam-web-adapter/     # IamWebRequestContextResolver, IamApiKeyLookupService, IamOAuthTokenLookupService, IamOpenApiWebRequestContextResolver
  sdkwork-iam-web-adapter/     # IamDomainContextInjector, IamAuthorizationPolicy
  sdkwork-routes-iam-*         # 挂载 with_web_request_context(adapter_runtime)
```

**所有 IAM 相关代码留在 appbase；框架只定义 trait 与调用时机。**

## 10. 验收：是否违反基础框架定位

- [x] `cargo tree` 无指向 `sdkwork-appbase`、`sdkwork-iam-*`、`sdkwork-claw-*` 的依赖（architecture test 守护）
- [x] 框架 crate 内无 `iam_` 表名、无 OAuth、无业务 operationId（sqlx_migrations 测试守护）
- [x] `WebRequestContextResolver` 仅有 trait + dev stub；生产 JWT 验签通过 `TenantBoundJwtVerifier` + `TenantSigningKeyLookup`（EP-05d，HS256/RS256）；SaaS 吊销通过 EP-05e；IAM 全量 resolver 由业务 adapter 实现
- [x] 领域类型注入仅通过 `DomainContextInjector`
- [x] 业务 route crate 均在业务仓库，不在 web-framework（control-plane 例外已 ADR 文档化）

## 11. 极致标准体系（rev.3）

| 文档 | 内容 |
| --- | --- |
| [TECH-12-industry-framework-benchmark.md](./TECH-12-industry-framework-benchmark.md) | Spring / ASP.NET / Nest / Stripe 对标 |
| [TECH-13-capability-catalog.md](./TECH-13-capability-catalog.md) | **80+ 功能点与技术点**（A–L 域） |
| [TECH-14-standards-system.md](./TECH-14-standards-system.md) | L0–L3 四层标准金字塔 |
| [TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md) | 20 个扩展点 EP-01…EP-20 |
| [TECH-16-maturity-model.md](./TECH-16-maturity-model.md) | M0–M4 成熟度与 GA 门槛 |
| [specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md) | 可评审框架标准正文 |
| [specs/web-framework-capability.matrix.json](../specs/web-framework-capability.matrix.json) | 机器可读能力矩阵 |

**North Star**：业务集成 ≤3 步；Handler 零凭证 Header 解析；18 阶段不可绕过；`cargo tree` 零业务依赖。

## 12. 文档阅读顺序

1. 本文档（基础定位）
2. [TECH-12-industry-framework-benchmark.md](./TECH-12-industry-framework-benchmark.md) + [TECH-14-standards-system.md](./TECH-14-standards-system.md)
3. [TECH-13-capability-catalog.md](./TECH-13-capability-catalog.md) + [TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md)
4. [TECH-02-architecture-design.md](./TECH-02-architecture-design.md)
5. [TECH-03-web-request-context.md](./TECH-03-web-request-context.md) · [TECH-04-pipeline-interceptor-design.md](./TECH-04-pipeline-interceptor-design.md)
6. [TECH-10-migration-from-appbase.md](./TECH-10-migration-from-appbase.md)（业务侧迁移）

