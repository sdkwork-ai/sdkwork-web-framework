> Migrated from `docs/architecture/tech/TECH-19-java-spring-filter-parity.md` on 2026-06-24.
> Owner: SDKWork maintainers

# Java / Spring Filter 链对齐指南

> 与 Rust `WebCallInterceptorChain` 1:1 阶段名对齐。共享向量：[`tests/vectors/pipeline-stage-order.json`](../tests/vectors/pipeline-stage-order.json)。

## 1. 目标

SDKWork Java/Spring HTTP 后端 **不得** 重新实现 competing context 框架。Filter / HandlerInterceptor 顺序 **必须** 与 Rust 18 阶段一致，语义与 [`WEB_FRAMEWORK_SPEC.md`](../../sdkwork-specs/WEB_FRAMEWORK_SPEC.md) 及 [`specs/WEB_FRAMEWORK_STANDARD.md`](../specs/WEB_FRAMEWORK_STANDARD.md) 对齐。

## 2. 阶段映射

| 顺序 | Rust `WebCallStage` | Java Filter 名（规范） | 职责摘要 |
| --- | --- | --- | --- |
| 1 | RequestIdentity | `RequestIdentityFilter` | 服务端 `X-Request-Id` |
| 2 | SurfaceClassification | `SurfaceClassificationFilter` | app/backend/open/gateway 面分类 |
| 3 | Cors | `CorsFilter` | CORS 预检与 allowlist |
| 4 | MethodGuard | `MethodGuardFilter` | HTTP 方法白名单 |
| 5 | CrossSiteRequest | `CrossSiteRequestFilter` | CSRF / Origin |
| 6 | SqlInjectionGuard | `SqlInjectionGuardFilter` | Header SQL 启发式 |
| 7 | RequestSizeLimit | `RequestSizeLimitFilter` | Content-Length / body cap |
| 8 | RateLimit | `RateLimitFilter` | 分布式/本地限流 |
| 9 | Idempotency | `IdempotencyFilter` | 幂等键与重放 |
| 10 | RequestContextResolution | `RequestContextResolutionFilter` | Resolver trait 调用 |
| 11 | Authentication | `AuthenticationFilter` | 凭证 → principal |
| 12 | Authorization | `AuthorizationFilter` | `AuthorizationPolicy` |
| 13 | TenantIsolation | `TenantIsolationFilter` | 租户边界 |
| 14 | ContextInjection | `ContextInjectionFilter` | `WebRequestContext` + injectors |
| 15 | Logging | `LoggingFilter` | 结构化日志（脱敏） |
| 16 | Audit | `AuditFilter` | 审计 fact |
| 17 | HeaderSecurity | `HeaderSecurityFilter` | 安全响应头 |
| 18 | ResponseIdentity | `ResponseIdentityFilter` | 响应 request-id |

## 3. Java 实现边界

| 项 | 规则 |
| --- | --- |
| 上下文类型 | `WebRequestContext` method parameter / `@RequestAttribute`，禁止 Controller 解析 Header |
| 错误体 | RFC 7807 `application/problem+json` |
| 公开路由 | manifest `RouteAuth::Public`；仍跑全链，跳过 10–13 凭证/授权 |
| 领域上下文 | `DomainContextInjector` Spring bean 列表 |
| 生产装配 | 等价于 Rust `validate_production_assembly` 检查（非 NoOp emitter、Redis HA store 等） |

## 4. 参考骨架（Spring Boot 3）

```java
@Configuration
public class SdkworkWebFrameworkFilterConfig {
  @Bean
  FilterRegistrationBean<RequestIdentityFilter> requestIdentity() { /* order 1 */ }
  // … 18 filters in STANDARD_STAGE_ORDER …
  @Bean
  FilterRegistrationBean<ResponseIdentityFilter> responseIdentity() { /* order 18 */ }
}
```

Filter `order` 值 **必须** 与上表顺序单调递增；单元测试读取 `pipeline-stage-order.json` 做 parity 断言（与 Rust `java_parity` test 同源）。

## 5. 验证

| 证据 | 命令 / 路径 |
| --- | --- |
| Rust 向量权威 | `cargo test -p sdkwork-web-architecture-tests --test java_parity` |
| Java parity（业务仓库） | JUnit 读取 classpath `sdkwork/pipeline-stage-order.json` |
| 跨语言安全向量 | 共享 `tests/vectors/forged-identity-headers.json` |

## 6. 本仓库范围

Java Filter 实现 **不在** `sdkwork-web-framework`  crate 树内；本文件与共享 JSON 向量为 Java 平行运行时的 **契约输入**。实现归属业务仓库或未来 `sdkwork-web-framework-java` 组件（需 ADR）。

