> Migrated from `docs/architecture/tech/TECH-11-tech-stack-selection.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 技术选型

> 前置阅读：[TECH-00-framework-foundation.md](./TECH-00-framework-foundation.md)

## 1. 定位

技术选型服务于 **Web 运行时集成封装**，与 `sdkwork-clawrouter` 对齐，便于业务仓库统一栈；**选型结果落在框架 crate 内**，不引入业务依赖。

## 2. 核心技术栈

| 层级 | 选型 | 框架 crate |
| --- | --- | --- |
| HTTP | Axum 0.8 | `sdkwork-web-axum`, `sdkwork-web-bootstrap` |
| 中间件 | Tower 0.5, tower-http 0.6 | `sdkwork-web-axum` |
| 异步 | Tokio 1.5x | axum / store |
| 序列化 | serde | `sdkwork-web-core`, `sdkwork-web-contract` |
| 观测 | tracing | `sdkwork-web-core` Logging stage |
| 出站 HTTP | Hyper 1.x | **不在核心框架**；业务 gateway 自用 |
| SQL（可选） | sqlx 0.8 | **仅** `sdkwork-web-store-sqlx` |
| 缓存（可选） | redis | **仅** `sdkwork-web-store-redis` |

## 3. Crate 与依赖隔离

| Crate | axum | sqlx | redis |
| --- | --- | --- | --- |
| sdkwork-web-contract | ❌ | ❌ | ❌ |
| sdkwork-web-core | ❌ | ❌ | ❌ |
| sdkwork-web-axum | ✅ | ❌ | ❌ |
| sdkwork-web-bootstrap | ✅ | ❌ | ❌ |
| sdkwork-web-store-sqlx | ❌ | ✅ | ❌ |

## 4. 与 claw-router

- claw-router **消费**本框架 crate，版本与 workspace 对齐
- `InvocationPipeline`、Hyper 出站留在 claw-product/gateway，**不并入** web-framework 核心

## 5. 禁止

- 框架 workspace 依赖 `sdkwork-claw-*`、`sdkwork-appbase`、`sdkwork-iam-*`
- 在 `sdkwork-web-core` 引入 sqlx

## 6. 验证

```bash
cargo tree --workspace -i axum
# 架构测试：tests/architecture/no_business_dependencies.rs
```

