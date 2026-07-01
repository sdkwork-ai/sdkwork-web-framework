> Migrated from `docs/architecture/tech/TECH-18-owasp-api-top10-mapping.md` on 2026-06-24.
> Owner: SDKWork maintainers

# OWASP API Security Top 10 映射

> 框架层面对 [OWASP API Security Top 10 (2023)](https://owasp.org/API-Security/) 的覆盖映射。  
> 业务层授权、数据域、供应链等仍需应用仓库自行闭合。

## 1. 映射表

| OWASP ID | 风险 | 框架阶段 / 能力 | 证据 |
| --- | --- | --- | --- |
| API1:2023 | Broken Object Level Authorization | Stage 12 Authorization + Stage 13 TenantIsolation + `path_resource_guard` | `security_vectors`: tenant path mismatch, ambient tenant path |
| API2:2023 | Broken Authentication | Stage 10 RequestContextResolution + Stage 11 Authentication | JWT tenant-bound verify, session revocation, dual-token |
| API3:2023 | Broken Object Property Level Authorization | `WebRequestContext.scopes` + handler `RequirePrincipal` | B3 data_scope / permission_scope vocabulary |
| API4:2023 | Unrestricted Resource Consumption | Stage 7 RequestSizeLimit + Stage 8 RateLimit + D9 concurrent admission | 413, 429 + Retry-After, tenant runtime profile |
| API5:2023 | Broken Function Level Authorization | Stage 12 `AuthorizationPolicy` + manifest `required_permission` | `ManifestAuthorizationPolicy`, `DenyAllAuthorizationPolicy` |
| API6:2023 | Unrestricted Access to Sensitive Business Flows | Stage 8 `RateLimitTier::AuthCritical` + idempotency | auth_critical tier tests, Idempotency-Key |
| API7:2023 | Server Side Request Forgery | **不在 HTTP 框架** | 业务出站 HTTP 客户端 / 网关策略 |
| API8:2023 | Security Misconfiguration | `validate_production_assembly` + C15 CORS prod lint | 拒绝 dev resolver、NoOp emitters、unsafe CORS |
| API9:2023 | Improper Inventory Management | Route manifest + OpenAPI materialization | `HttpRoute`, openapi authority tests |
| API10:2023 | Unsafe Consumption of APIs | **不在 HTTP 框架** | 应用 SDK / 出站集成规范 |

## 2. 横切控制

| 控制 | 阶段 / 模块 | OWASP 关联 |
| --- | --- | --- |
| 客户端 tenant/user 头拒绝 | B9 `reject_client_identity_projection` | API1, API2, API3 |
| 伪造头 fuzz 向量 | `tests/vectors/forged-identity-headers.json` | API1, API8 |
| CSRF cookie 策略 | Stage 5 CrossSiteRequest | API2, API8 |
| SQL 注入 guard（header） | Stage 6 SqlInjectionGuard | API8 |
| 凭证日志脱敏 | E5/C8 `init_tracing` + `redact_sensitive_log_value` | API8 |
| Problem+json 无栈泄露 | G6 `problem_response` | API8 |
| 安全响应头 | Stage 17 HeaderSecurity | API8 |
| 审计 / 安全事件 | Stage 16 Audit + `SecurityEventEmitter` | API9, 合规追溯 |

## 3. 生产装配必选项（API8）

`validate_production_assembly` 在 `production_defaults()` 路径上强制：

- 租户绑定 JWT 验证 resolver（非 dev / 非 global HS256）
- 显式 `AuthorizationPolicy` 与 `TenantIsolationPolicy`
- 非 NoOp `AuditEmitter` / `SecurityEventEmitter`
- SaaS：`RedisRateLimitStore`、`RedisIdempotencyStore`、`RedisConcurrentAdmissionStore`
- SaaS：`ReadinessCheck`、`JwtProductionClaimPolicy::saas_production()`
- CORS：`allow_all_origins + allow_credentials` 拒绝

## 4. 验证

| 证据 | 命令 / 路径 |
| --- | --- |
| 安全向量 | `cargo test -p sdkwork-web-architecture-tests security_vectors` |
| 伪造头 fuzz | `cargo test -p sdkwork-web-architecture-tests header_fuzz` |
| 生产 CORS lint | `production_assembly` + admin CORS upsert tests |
| 共享阶段向量 | `tests/vectors/pipeline-stage-order.json` + `java_parity` test |

## 5. 业务层责任（框架不替代）

- API1/API3/API5：业务 `AuthorizationPolicy`、repository 租户过滤、数据域规则
- API7/API10：出站 HTTP/gRPC 客户端 allowlist 与凭证隔离
- 密钥轮换、WAF、mTLS、SBOM：见 `sdkwork-specs/SECURITY_SPEC.md`、`SUPPLY_CHAIN_SECURITY_SPEC.md`

