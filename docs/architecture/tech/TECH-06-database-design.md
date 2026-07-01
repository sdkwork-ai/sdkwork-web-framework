> Migrated from `docs/architecture/tech/TECH-06-database-design.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 数据库设计

> 前置阅读：[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)

## 1. 范围

仅 **框架运行时表**（`web_*`），定义于 `sdkwork-web-store-sqlx`（可选 crate）。

| 包含 | 不包含 |
| --- | --- |
| `web_rate_limit_policy` | `iam_*` |
| `web_idempotency_record` | 产品业务表 |
| `web_audit_event`（框架审计通道） | IAM 登录会话表 |
| `web_security_event` | |
| `web_cors_policy` | |
| `web_tenant_runtime_profile` | |

**IAM 表留在 appbase**；框架 store **不得** JOIN `iam_user` 等。

## 2. 设计原则

- 遵循 `DATABASE_SPEC.md` `tenant_entity` 模式
- 框架 SQL migration 在 `sdkwork-web-store-sqlx/migrations/`
- 业务可选用 **内存 store**（`sdkwork-web-core`）而不启用 sqlx crate

## 3. 表清单（摘要）

### web_rate_limit_policy

租户/平台级流控策略模板。见前版 §3.1 字段定义。

### web_idempotency_record

幂等键 + request_hash + 响应快照 + TTL。

### web_audit_event

框架 `AuditEmitter` 默认落库；**业务 IAM 审计可另写 `iam_audit_*` 或复用 emitter 实现双写**（在 appbase adapter，非框架）。

### web_security_event

CORS 拒绝、流控触发等安全事件。

### web_cors_policy

按 tenant + environment 的 CORS 配置。

### web_tenant_runtime_profile

租户 Web 运行时开关（流控开关、body 限额、open_api_prefixes 等）。

## 4. Store trait（框架）

```rust
pub trait RateLimitStore: Send + Sync { ... }
pub trait IdempotencyStore: Send + Sync { ... }
pub trait AuditEmitter: Send + Sync { ... }  // 可落 web_audit_event 或 noop
```

- `MemoryRateLimitStore` → `sdkwork-web-core`
- `SqlxRateLimitStore` → `sdkwork-web-store-sqlx`（仅 `web_*`）

## 5. Redis（可选 sdkwork-web-store-redis）

流控热路径；键规则见 [TECH-07-security-standards.md](./TECH-07-security-standards.md)。

## 6. 业务数据访问规则

业务 repository：

- 从 `WebRequestContext` / `WebRequestPrincipal` 取 `tenant_id`
- **不得**从框架 store 表读取业务真值

## 7. 验收

- [x] migration 仅含 `web_*`（`sqlx_migrations` architecture test）
- [x] `cargo tree -p sdkwork-web-store-sqlx` 无 iam 依赖（`dependency_graph`）
- [x] 架构测试禁止 framework SQL 出现 `iam_` 前缀（`sqlx_migrations`）

