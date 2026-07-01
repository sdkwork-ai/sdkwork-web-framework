# sdkwork-web-framework 设计文档

**所有带 HTTP API 的 SDKWork 能力仓库所依赖的 Web/SaaS 基础底层框架** — 零业务依赖；对 Axum/Tower 集成封装；制定标准并提供可插拔通用能力。

## 必读路径

1. [TECH-00-framework-foundation.md](./architecture/tech/TECH-00-framework-foundation.md) — 是什么 / 不是什么
2. [TECH-14-standards-system.md](./architecture/tech/TECH-14-standards-system.md) — **标准体系四层金字塔**
3. [TECH-13-capability-catalog.md](./architecture/tech/TECH-13-capability-catalog.md) — **80+ 功能点与技术点**
4. [TECH-12-industry-framework-benchmark.md](./architecture/tech/TECH-12-industry-framework-benchmark.md) — Spring / ASP.NET / Nest / Stripe 对标

## 标准产物

| 产物 | 路径 |
| --- | --- |
| 框架标准正文 | [specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md) |
| 能力矩阵 JSON | [specs/web-framework-capability.matrix.json](../specs/web-framework-capability.matrix.json) |
| 扩展点注册表 | [TECH-15-extension-points-registry.md](./architecture/tech/TECH-15-extension-points-registry.md) |
| 成熟度 M0–M4 | [TECH-16-maturity-model.md](./architecture/tech/TECH-16-maturity-model.md) |
| OWASP API Top 10 映射 | [TECH-18-owasp-api-top10-mapping.md](./architecture/tech/TECH-18-owasp-api-top10-mapping.md) |
| 生产 Rollout / 采纳证据 | [TECH-24-production-rollout-and-adoption.md](./architecture/tech/TECH-24-production-rollout-and-adoption.md) |
| Java Filter 1:1 对齐 | [TECH-19-java-spring-filter-parity.md](./architecture/tech/TECH-19-java-spring-filter-parity.md) |
| Release pipeline benchmark | `scripts/benchmark-pipeline.ps1` / `.sh`（可选 `VERIFY_RELEASE_BENCH=1` 纳入 verify） |

## 依赖关系

```text
sdkwork-specs (L0) → sdkwork-web-framework (L1/L2) → appbase / claw-router / …
```

## 完整索引

[TECH-00-design-index.md](./architecture/tech/TECH-00-design-index.md)

## 状态

- 设计：**rev.3**（行业标准 + 极致标准体系）
- 实现：**M3 核心能力**（能力矩阵 `currentMaturity: M3`）；M4 rollout / 多产品采纳见 [TECH-24-production-rollout-and-adoption.md](./architecture/tech/TECH-24-production-rollout-and-adoption.md)
- 安全映射：[TECH-18-owasp-api-top10-mapping.md](./architecture/tech/TECH-18-owasp-api-top10-mapping.md)

## Canon Documents

| Document | Path |
| --- | --- |
| Product PRD | [product/prd/PRD.md](product/prd/PRD.md) |
| Technical architecture | [architecture/tech/TECH_ARCHITECTURE.md](architecture/tech/TECH_ARCHITECTURE.md) |
