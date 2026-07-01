> Migrated from `docs/architecture/tech/TECH-09-ui-ux-functional-plan.md` on 2026-06-24.
> Owner: SDKWork maintainers

# UI/UX 与功能规划

> 前置阅读：[TECH-00-framework-foundation.md](../../architecture/tech/TECH-00-framework-foundation.md)

## 1. 范围说明

**`sdkwork-web-framework` 不包含 `apps/` UI 实现。**

本文档描述：

- 基于框架标准 **可构建** 的运维/开发者体验（供并行 apps 或其它 **产品仓库** 实现）
- 框架提供的 **契约与数据基础**（`web_*` 表、Audit/Security 事件）

## 2. 控制台归属（修订）

| 能力 | 推荐归属 | 说明 |
| --- | --- | --- |
| IAM 用户/角色/组织 | appbase backend-ui | 已有 IAM 域 |
| Web 流控/CORS/安全事件 | platform-admin 或 appbase 扩展模块 | **消费** `web_*` 与框架 emitter |
| 框架文档/集成指南 | web-framework `docs/` + 静态门户 | 无业务 API |

**不在 web-framework 仓库内建 admin API 路由**（见 [TECH-05-api-surface-design.md](../../architecture/tech/TECH-05-api-surface-design.md) §8）。

## 3. 可规划界面（实现方自选）

### 3.1 Web 安全中心（platform 或 appbase admin）

- CORS 策略 CRUD → `web_cors_policy`
- 流控策略 CRUD → `web_rate_limit_policy`
- 安全事件列表 → `web_security_event`

### 3.2 审计浏览器

- 框架审计 → `web_audit_event`
- IAM 审计 → appbase 表（联合查询在 **业务 UI** 组装）

### 3.3 开发者门户（文档站）

- WebRequestContext 字段说明
- 18 阶段链图解
- 业务集成三步：依赖 crate → 实现 Resolver → 挂载 Layer

## 4. UX 原则

- 租户切换在 **backend-ui** 标准顶栏
- 策略变更写审计
- prod CORS 修改二次确认
- 不展示 raw token

## 5. 与并行 apps 进程的接口

| 交付物 | 位置 |
| --- | --- |
| 设计规格 | 本文档 + `03-web-request-context.md` |
| 数据模型 | `06-database-design.md` |
| Rust trait | `sdkwork-web-core` |
| 业务 HTTP（若需要） | **platform-admin 产品仓库**，非 framework |

## 6. 路线图

| 阶段 | 内容 | 仓库 |
| --- | --- | --- |
| P0 | 文档门户 + 集成指南 | web-framework docs |
| P1 | Web 安全中心 UI | platform-admin 或 appbase admin |
| P2 | Request trace 视图 | 依赖 observability 产品化 |

