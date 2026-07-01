> Migrated from `docs/adr/2026-06-17-web-framework-repository-extraction.md` on 2026-06-24.
> Owner: SDKWork maintainers

# ADR-001: sdkwork-web-framework 作为零业务依赖的 Web 基础框架

- **Status**: Accepted (rev. 2)
- **Date**: 2026-06-17
- **Supersedes**: ADR-001 rev.1（「平台层」表述易与产品混淆）

## Context

SDKWork 需要统一的 SaaS Web 开发标准与 Axum 集成封装。原方案将部分能力放在 `sdkwork-appbase` 并与 IAM 耦合，且设计中曾出现框架自有业务 API、框架侧 IAM bridge 等 **反向依赖或职责混淆**。

约束：

- 所有带 API 的产品仓库应共享同一框架
- `sdkwork-appbase` 是 IAM **业务**能力仓库，应 **依赖**框架而非定义框架
- 框架不得依赖任何业务代码

## Decision

1. **`sdkwork-web-framework` 定位为 L1 基础框架**：Web 集成封装 + SaaS 标准 + 通用能力 trait/默认实现
2. **单向依赖**：`sdkwork-appbase`、`sdkwork-clawrouter` 等 → `sdkwork-web-framework`；禁止反向
3. **框架 crate 拆分**：contract / context / pipeline / security / axum / bootstrap / store（可选）
4. **`WebRequestContext`** 为框架核心类型；IAM 映射仅在 appbase `sdkwork-iam-web-adapter`
5. **扩展点**：`WebRequestContextResolver`、`AuthorizationPolicy`、`DomainContextInjector` 等由业务实现
6. **路由与 OpenAPI authority** 留在业务仓库；框架不提供业务 backend-api
7. 从 appbase 迁出 `sdkwork-platform-http-context-service` 时 **剥离全部 IAM import**

## Alternatives Rejected

| 方案 | 拒绝原因 |
| --- | --- |
| 框架留在 appbase | 非 IAM 产品被迫依赖 IAM 仓库 |
| 框架依赖 iam-context 做注入 | 违反零业务依赖 |
| 框架内置 IAM backend-api | 混淆平台框架与 IAM 产品 |
| claw-router 与框架并列两套 HTTP 基座 | 重复维护 |

## Consequences

**Positive**

- 清晰的依赖金字塔
- AIoT/T1 commerce repos 可无 appbase 依赖框架
- 安全链、上下文标准一处实现

**Negative**

- 跨仓库 path 依赖管理
- appbase 需 adapter 层一次性投入

## Compliance

见 [TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md) §10 验收清单。

## References

- [TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)
- [TECH-02-architecture-design.md](./TECH-02-architecture-design.md)
- `sdkwork-specs/API_SPEC.md` §10
- `sdkwork-specs/WEB_BACKEND_SPEC.md`

