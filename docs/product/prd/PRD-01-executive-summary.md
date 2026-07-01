> Migrated from `docs/01-executive-summary.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 执行摘要

## 1. 背景

SDKWork 各带 API 的产品共享 HTTP 契约（`/app/v3/api`、`/backend/v3/api`、领域 open-api）与 SaaS 多租户模型。通用 Web 能力曾落在 `sdkwork-appbase`，并与 IAM 耦合；`sdkwork-clawrouter` 等又重复实现 Axum 中间件。缺少 **独立的、零业务依赖的 Web 基础框架**。

## 2. 定位（修订）

**`sdkwork-web-framework` = 所有带 API 能力仓库的 Web/SaaS 基础底层框架。**

- **是**：Axum/Tower 集成封装、SaaS 标准、通用能力（上下文、Pipeline、安全、观测基座）
- **不是**：IAM 服务、产品路由、业务数据库、应用前端
- **依赖关系**：`sdkwork-appbase` → `sdkwork-web-framework`（单向）

## 3. 目标

1. **标准**：固化 SaaS Web 开发规范（接口面、上下文、调用链、错误、安全挂载点）
2. **封装**：业务只装配 `WebFrameworkRuntime`，不手写 CORS/流控/上下文链
3. **扩展**：Resolver、Authorization、ApiKeyLookup、OAuthTokenLookup、OpenApiCredentialSchemeDetector、DomainContextInjector 由业务实现
4. **迁出**：自 appbase 抽出原 `sdkwork-platform-http-context-service`，去除 IAM 硬依赖
5. **对齐技术栈**：Axum 0.8 + Tower（与 claw-router 一致），claw-router **消费**本框架而非并列重复

## 4. 非目标

- 不实现 `apps/` 前端
- 不在本仓库放置业务 `sdkwork-routes-<产品>-*`（框架 control-plane 例外：`sdkwork-routes-web-framework-backend-api`，见 `WEB_FRAMEWORK_STANDARD.md` §8）
- 不实现 IAM/电商/网关等业务 API
- 不依赖任何业务 crate

## 5. 关键决策

| 决策 | 选择 |
| --- | --- |
| 生态位 | Layer 1 基础框架，业务在其上 |
| 依赖 | 业务 → 框架；框架 ↛ 业务 |
| 上下文 | `WebRequestContext`（框架类型，与 API_SPEC 词汇对齐） |
| 领域上下文 | `DomainContextInjector` trait，IAM 适配在 appbase |
| 认证实现 | `WebRequestContextResolver` trait，生产实现仅在业务侧 |
| Crate 拆分 | contract / core / axum / bootstrap / store（可选） / routes-web-framework-backend-api（control-plane） |
| 路由所有权 | 业务产品在业务仓库；本仓库仅 framework control-plane backend-api |

## 6. 框架 Crate 一览（目标）

| Crate | 职责 |
| --- | --- |
| `sdkwork-web-contract` | `ApiSurface`、`HttpRoute`、`HttpMethod`（无 axum） |
| `sdkwork-web-core` | `WebRequestContext`、Resolver trait、18 阶段 Interceptor 链、SecurityPolicy、Store trait 与内存默认实现 |
| `sdkwork-web-axum` | middleware、extractor、Layer |
| `sdkwork-web-bootstrap` | healthz、metrics、contract_fallback（自动挂载）、service_router、`WebFramework::builder` |
| `sdkwork-web-store-sqlx` | 可选：`web_*` 表 sqlx 实现（仍无业务表） |

## 7. 成功标准

### 框架仓库（已达成）

- [x] 框架 workspace `cargo tree` 无业务依赖
- [x] 18 阶段链在框架内完整可测（store 可内存实现；K1–K7 + release benchmark）

### 业务消费者（Phase B–E，见 [TECH-10-migration-from-appbase.md](../../architecture/tech/TECH-10-migration-from-appbase.md)）

- [ ] appbase 通过 adapter crate 实现 Resolver/Injector，并依赖框架
- [ ] appbase 删除 `sdkwork-platform-http-context-service`
- [ ] claw-router 可改为依赖 `sdkwork-web-bootstrap` + `sdkwork-web-axum`

## 8. 极致标准体系（rev.3 新增）

| 维度 | 文档 |
| --- | --- |
| 行业对标 | [TECH-12-industry-framework-benchmark.md](../../architecture/tech/TECH-12-industry-framework-benchmark.md) |
| 80+ 能力项 | [TECH-13-capability-catalog.md](../../architecture/tech/TECH-13-capability-catalog.md)（A–L 域） |
| 标准金字塔 | [TECH-14-standards-system.md](../../architecture/tech/TECH-14-standards-system.md) |
| 20 扩展点 | [TECH-15-extension-points-registry.md](../../architecture/tech/TECH-15-extension-points-registry.md) |
| M0–M4 成熟度 | [TECH-16-maturity-model.md](../../architecture/tech/TECH-16-maturity-model.md) |
| 可评审标准 | [specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md) |

**North Star**：`WebFramework::builder()` 三步集成；18 阶段管道不可绕过；Handler 零 Header 鉴权样板。

## 9. 文档索引

[TECH-00-design-index.md](../../architecture/tech/TECH-00-design-index.md)

