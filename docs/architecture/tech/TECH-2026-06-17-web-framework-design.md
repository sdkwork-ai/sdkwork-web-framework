> Migrated from `docs/superpowers/specs/2026-06-17-web-framework-design.md` on 2026-06-24.
> Owner: SDKWork maintainers

# sdkwork-web-framework 总体设计规格

> Superpowers 设计规格 · rev.3 · 2026-06-17

## 摘要

零业务依赖的 SDKWork Web/SaaS **基础底层框架**：对标 Spring/ASP.NET/Nest 管道 + Stripe 幂等限流，以 Rust Trait + 18 阶段 Interceptor 封装 Axum/Tower。

## 标准体系

- **L0** `sdkwork-specs`
- **L1** `specs/WEB_FRAMEWORK_STANDARD.md` + capability matrix
- **L2** 框架 runtime enforcement
- **L3** 业务 adapter（appbase `sdkwork-iam-web-adapter` 等）

## 能力规模

- **12 能力域** A–L
- **80+ 功能/技术点**（见 docs/13）
- **20 扩展点** EP-01…EP-20
- **成熟度 M0–M4**（GA 门槛见 docs/16）

## 集成极致目标

```rust
WebFramework::builder().resolver(...).build().layer(business_router);
```

## 下一步

Phase I：M3 核心能力实现 + 架构测试（无业务依赖）

## 文档

[TECH-00-design-index.md](./TECH-00-design-index.md)

