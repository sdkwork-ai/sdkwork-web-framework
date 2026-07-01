> Migrated from `docs/architecture/tech/TECH-07-security-standards.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 安全标准体系

> 前置阅读：[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)  
> 权威：`sdkwork-specs/SECURITY_SPEC.md` §5.1；本文件描述 **框架如何封装 enforcement**。

## 1. 安全分层

```text
Edge (TLS/WAF) — 部署层，非框架 crate
  → Framework Interceptor Chain（本仓库强制执行）
  → AuthorizationPolicy / TenantIsolationPolicy（业务 trait 实现）
  → Repository 参数绑定（业务层）
```

## 2. 框架封装的安全能力

| 能力 | 模块 | 默认行为 |
| --- | --- | --- |
| CORS | `sdkwork-web-core` Cors stage | deny-by-default |
| 跨站请求防护 | CrossSiteRequest stage | 状态变更方法校验 Origin |
| 方法白名单 | MethodGuard | GET/POST/PUT/PATCH/DELETE/OPTIONS |
| 请求体大小 | RequestSizeLimit | 可配置，默认 16MB |
| SQL 注入启发式 | SqlInjectionGuard | 可配置 header |
| 流控 | RateLimit + `RateLimitStore` | 内存/Redis/SQL 可插拔 |
| 幂等 | Idempotency + `IdempotencyStore` | 可插拔 |
| 安全响应头 | HeaderSecurity | nosniff, frame deny, referrer |
| 请求 ID | RequestIdentity / ResponseIdentity | 服务端 UUID |
| 凭证解析时机 | 在安全 guard **之后** | API_SPEC 要求 |

OWASP API Top 10 框架层映射见 [TECH-18-owasp-api-top10-mapping.md](./TECH-18-owasp-api-top10-mapping.md)。

## 3. 业务负责的安全能力

| 能力 | 实现位置 |
| --- | --- |
| Token 验签、撤销 | `WebRequestContextResolver`（appbase adapter） |
| RBAC / permission_scope | `AuthorizationPolicy`（appbase adapter） |
| API Key 记录与 hash | `ApiKeyLookupService`（appbase adapter） |
| OAuth Bearer token/session 查表 | `OAuthTokenLookupService`（appbase adapter） |
| Open-api 凭证 scheme 选择 | `OpenApiCredentialSchemeDetector`（框架默认 + 业务可覆盖） |
| 业务数据租户隔离 SQL | 业务 repository |
| CSRF token（cookie 流） | 可选 `CsrfPolicy` + 业务 session |

## 4. CORS / 跨站

- CORS 与 CSRF **独立**；框架分别封装
- 配置：`SecurityPolicy.cors` + 可选 `web_cors_policy` 表（store crate）
- prod 禁止 `allow_all_origins`

## 5. 流控

- 框架提供 **引擎 + store trait**
- 策略维度：IP、tenant、user、operation（键哈希，无 PII）
- IAM 登录流控 **应**迁移到框架 `RateLimitStore`，删除 handler 内 ad hoc 实现（在 appbase 改造，非框架写 IAM SQL）

## 6. 日志与审计

- `Logging` stage：tracing，脱敏
- `Audit` stage：调用 `AuditEmitter`
- 默认 `SqlxAuditEmitter` 写 `web_audit_event`；IAM 业务审计由 appbase 自定义 emitter **组合**或替换

## 7. 威胁模型

| 威胁 | 框架层防护 |
| --- | --- |
| 伪造租户头 | 忽略投影头；仅 resolver 填充 principal |
| 跨域滥用 | CORS + CrossSite |
| 暴力破解 | RateLimit（业务配置敏感路径 tier） |
| 重放 | Idempotency + token exp（exp 在 resolver） |

## 8. 验收

### 框架仓库（Phase A — 已完成）

- [x] 安全策略均可通过 `SecurityPolicy` 配置，无需改 Handler
- [x] 框架测试覆盖 CORS deny、rate limit 429（`security_vectors` / `header_fuzz` / `pipeline_stress`）
- [x] 无业务表出现在 security crate（`sqlx_migrations` 架构测试）

