> Migrated from `docs/architecture/tech/TECH-14-standards-system.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 标准体系（Standard System）

> sdkwork-web-framework 的 **标准体系** = 全局 specs + 本仓库标准 + 框架强制执行 + 业务扩展契约。

## 1. 四层标准金字塔

```text
                    ┌─────────────────────┐
                    │  L0  sdkwork-specs   │  法律层：API/IAM/DB/Security/Obs…
                    └──────────┬──────────┘
                               │ 引用
                    ┌──────────▼──────────┐
                    │  L1  WEB_FRAMEWORK   │  框架标准：本仓库 specs/ + docs/
                    │       STANDARD       │
                    └──────────┬──────────┘
                               │ 编码 enforcement
                    ┌──────────▼──────────┐
                    │  L2  Framework       │  运行时：18 阶段 / WebRequestContext
                    │       Runtime        │
                    └──────────┬──────────┘
                               │ 实现 trait
                    ┌──────────▼──────────┐
                    │  L3  Business        │  appbase adapter / 产品 router
                    │       Adapters       │
                    └─────────────────────┘
```

**原则**：下层不得 contradict 上层；L1 可 **窄化** L0，不可 **放宽** L0。

## 2. L0 全局规范映射（框架必须对齐）

| sdkwork-specs 文件 | 框架承接方式 |
| --- | --- |
| `WEB_FRAMEWORK_SPEC.md` | L0 强制集成：业务仓库必须依赖本仓库；Rust/Java 并行语义 |
| `API_SPEC.md` §10 | `WebRequestContext` 词汇；18 阶段顺序；三接口面 |
| `WEB_BACKEND_SPEC.md` | Handler 薄层；manifest 类型；禁止 raw header 解析 |
| `SECURITY_SPEC.md` §5.1 | Interceptor 基线表 → pipeline 实现 |
| `DATABASE_SPEC.md` | 仅 `web_*` tenant_entity 表 |
| `OBSERVABILITY_SPEC.md` | request_id、脱敏、route template 日志 |
| `CACHE_SPEC.md` | Redis 限流命名空间 |
| `CONFIG_SPEC.md` | `SDKWORK_WEB_*` 环境键（L1 定义） |
| `TEST_SPEC.md` | 契约测试、上下文测试、架构测试 |
| `NAMING_SPEC.md` | crate `sdkwork-web-*`；业务 `sdkwork-routes-*` |
| `MIGRATION_SPEC.md` | appbase 迁出流程 |

## 3. L1 框架专属标准（本仓库权威）

| 文档/产物 | 内容 |
| --- | --- |
| [TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md) | 依赖铁律、边界 |
| [TECH-03-web-request-context.md](./TECH-03-web-request-context.md) | 上下文标准 |
| [TECH-04-pipeline-interceptor-design.md](./TECH-04-pipeline-interceptor-design.md) | 管道标准 |
| [TECH-07-security-standards.md](./TECH-07-security-standards.md) | 安全封装标准 |
| [TECH-13-capability-catalog.md](./TECH-13-capability-catalog.md) | 能力目录 |
| [TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md) | 扩展点注册 |
| [TECH-16-maturity-model.md](./TECH-16-maturity-model.md) | 成熟度与 DoD |
| `specs/WEB_FRAMEWORK_STANDARD.md` | 可提交评审的框架标准摘要 |
| `specs/web-framework-capability.matrix.json` | 机器可读能力矩阵 |

## 4. L2 运行时强制执行清单

以下 **无业务配置即生效**（secure defaults）：

| # | 规则 | Enforcement |
| --- | --- | --- |
| R1 | 服务端生成 request_id | RequestIdentity |
| R2 | CORS deny-by-default | CorsPolicy::default() |
| R3 | 未认证 protected 路径 → 401 | Authentication |
| R4 | 超大 body → 413 | RequestSizeLimit |
| R5 | 非法方法 → 405 | MethodGuard |
| R6 | 响应安全头 | HeaderSecurity |
| R7 | 响应带 X-Request-Id | ResponseIdentity |
| R8 | 链错误 → Problem+json | WebFrameworkError |

以下 **必须显式装配**（无生产默认值）：

| # | 规则 | 装配 |
| --- | --- | --- |
| R9 | 生产 Token 验签 | `WebRequestContextResolver` |
| R10 | RBAC | `AuthorizationPolicy` |
| R11 | 租户隔离规则 | `TenantIsolationPolicy` |
| R12 | 生产限流后端 | `RateLimitStore` |

## 5. L3 业务扩展标准

业务 adapter **必须**：

| # | 要求 |
| --- | --- |
| B1 | 实现 `WebRequestContextResolver`，不得 bypass 链 |
| B2 | **每个** API Handler 参数列表含 `WebRequestContext`（`FromRequestParts` 自动注入） |
| B3 | protected Handler 使用 `require_tenant_id()` / `require_app_id()` |
| B4 | Service 传递 `&WebRequestContext` 或 `TenantAppContext` |
| B5 | `manifest.rs` 使用 `HttpRoute` + `RouteAuth` |
| B6 | OpenAPI **每个** operation：`x-sdkwork-request-context: WebRequestContext` |
| B7 | 敏感路径配置 `rate_limit_tier` |
| B8 | 领域上下文仅 `DomainContextInjector` |

## 6. 标准词汇表（跨语言）

| 词汇 | Rust | Java | OpenAPI extension |
| --- | --- | --- | --- |
| 请求上下文 | `WebRequestContext` | `WebRequestContext` | `x-sdkwork-request-context` |
| 接口面 | `WebApiSurface` | `WebApiSurface` | `x-sdkwork-api-surface` |
| 操作 | `operation_id` | `operationId` | `operationId` |
| 限流 tier | `RateLimitTier` | `RateLimitTier` | `x-sdkwork-rate-limit-tier` |
| 问题响应 | `WebFrameworkError` | `ProblemDetail` | `application/problem+json` |

## 7. 标准变更流程

1. 提案 → `docs/adr/`
2. 若触及 L0 → `sdkwork-specs` PR + 人工评审
3. 更新 capability matrix 版本
4. 框架契约测试更新
5. 业务 adapter 迁移指南（`MIGRATION`）

## 8. 合规检查（CI 目标）

```bash
# 框架仓库
cargo test --workspace
cargo test -p sdkwork-web-architecture-tests  # 无业务依赖
sdkwork-web-lint-manifest  # 若有：manifest 字段校验

# 业务仓库（集成后）
sdkwork-web-compliance check  # 未来：扫描 handler 是否解析 Authorization
```

## 9. 极致标准：「完美集成」定义

满足以下全部条件，称为 **Web Framework GA**：

- [x] L0 映射表 100% 有 L2 enforcement 或 L3 扩展点
- [x] capability catalog 核心项 ≥ M3（matrix `currentMaturity: M3`）
- [x] 架构测试禁止业务依赖
- [x] TestRuntime 覆盖 18 阶段集成测试
- [ ] appbase adapter 参考实现文档化（Phase B，跨仓库；框架侧见 [TECH-23-consumer-integration-template.md](./TECH-23-consumer-integration-template.md)）
- [x] Java parity checklist 完成（`java_parity` + `docs/architecture/tech/TECH-19-java-spring-filter-parity.md`）
- [x] 零 Handler 样板：业务 router 模板 < 20 行装配代码（[TECH-23-consumer-integration-template.md](./TECH-23-consumer-integration-template.md)）

## 10. 相关文档

- [TECH-12-industry-framework-benchmark.md](./TECH-12-industry-framework-benchmark.md)
- [TECH-16-maturity-model.md](./TECH-16-maturity-model.md)

