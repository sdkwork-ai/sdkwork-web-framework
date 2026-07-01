> Migrated from `docs/architecture/tech/TECH-00-design-index.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 设计文档索引

## 阅读顺序

### 所有人必读

1. **[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)** — 定位、依赖铁律、边界
2. **[TECH-14-standards-system.md](./TECH-14-standards-system.md)** — 四层标准金字塔
3. **[TECH-12-industry-framework-benchmark.md](./TECH-12-industry-framework-benchmark.md)** — 行业对标与封装哲学

### 极致标准（功能点 / 技术点 / 扩展点）

4. **[TECH-13-capability-catalog.md](./TECH-13-capability-catalog.md)** — A–L 域能力目录（80+ 项）
5. **[TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md)** — EP-01…EP-20
6. **[TECH-16-maturity-model.md](./TECH-16-maturity-model.md)** — M0–M4 与 GA 门槛
7. **[TECH-18-owasp-api-top10-mapping.md](./TECH-18-owasp-api-top10-mapping.md)** — OWASP API Top 10 框架映射
8. **[specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md)** — 框架标准正文
9. **[specs/web-framework-capability.matrix.json](../specs/web-framework-capability.matrix.json)** — 机器可读矩阵

### 跨运行时

10. **[TECH-19-java-spring-filter-parity.md](./TECH-19-java-spring-filter-parity.md)** — Java Filter 链 1:1 对齐指南

### 架构与实现

11. [01-executive-summary.md](./01-executive-summary.md)
10. [TECH-02-architecture-design.md](./TECH-02-architecture-design.md)
11. [TECH-11-tech-stack-selection.md](./TECH-11-tech-stack-selection.md)
12. **[TECH-03-web-request-context.md](./TECH-03-web-request-context.md)** — 结构定义 + 全自动注入 + 租户/应用持有
13. [TECH-04-pipeline-interceptor-design.md](./TECH-04-pipeline-interceptor-design.md)
14. **[TECH-17-websocket-standard.md](./TECH-17-websocket-standard.md)** — WebSocket 与 HTTP 统一 Pipeline / Context
15. [TECH-06-database-design.md](./TECH-06-database-design.md) · [TECH-07-security-standards.md](./TECH-07-security-standards.md)

### Crate 布局（实现）

| Crate | 状态 |
| --- | --- |
| `sdkwork-web-contract` | M3 |
| `sdkwork-web-core` | M3 |
| `sdkwork-web-axum` | M3 |
| `sdkwork-web-bootstrap` | M3 |
| `sdkwork-web-store-redis` | M3 |
| `sdkwork-web-store-sqlx` | M3 |
| `sdkwork-routes-web-framework-backend-api` | M3 |
| `sdkwork-web-admin-server` | M3 |
| `sdkwork-web-test-utils` | M3 |
| `sdkwork-web-schema-registry` | M2 |
| `apps/sdkwork-web-framework-pc` | M2 demo |

### 业务集成与运维

11. [TECH-10-migration-from-appbase.md](./TECH-10-migration-from-appbase.md) — appbase → web-framework 迁移
12. **[TECH-21-operations-runbook.md](./TECH-21-operations-runbook.md)** — 生产运维手册
13. **[TECH-22-bootstrap-and-routing.md](./TECH-22-bootstrap-and-routing.md)** — Builder / manifest / service_router
14. **[TECH-23-consumer-integration-template.md](./TECH-23-consumer-integration-template.md)** — 消费者 Rust 集成模板
15. **[TECH-24-production-rollout-and-adoption.md](./TECH-24-production-rollout-and-adoption.md)** — 生产 Rollout / M4 采纳证据
16. [TECH-05-api-surface-design.md](./TECH-05-api-surface-design.md) · [TECH-08-sdk-design.md](./TECH-08-sdk-design.md)

### UI（产品仓库实现）

17. [TECH-09-ui-ux-functional-plan.md](./TECH-09-ui-ux-functional-plan.md)

## 能力域速查（13）

| 域 | 名称 | 项数约 |
| --- | --- | --- |
| A | 请求生命周期 | 15 |
| B | SaaS 多租户 | 10 |
| C | 安全 | 12 |
| D | 韧性（限流/幂等） | 10 |
| E | 可观测性 | 10 |
| F | 契约治理 | 8 |
| G | 错误与响应 | 6 |
| H | 运行时引导 | 8 |
| I | 存储适配 | 7 |
| J | 验证与绑定 | 5 |
| K | 测试与质量 | 12 |
| L | 部署剖面 | 4 |

## 版本

- rev.3 · 2026-06-17 · 行业标准对标 + 标准体系 + 能力目录

