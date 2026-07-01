> Migrated from `docs/architecture/tech/TECH-16-maturity-model.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 成熟度模型（M0–M4）

> 用于 [TECH-13-capability-catalog.md](./TECH-13-capability-catalog.md) 每项能力的交付评判。

## 1. 等级定义

| 等级 | 名称 | 含义 | 验收 |
| --- | --- | --- | --- |
| **M0** | Documented | 标准已写入 docs/specs | 文档评审 |
| **M1** | Contracted | trait/类型/阶段存在；可无生产实现 | 编译 + 单元测试骨架 |
| **M2** | Functional | 内存/Noop 默认实现；TestRuntime 可跑通 | 集成测试 |
| **M3** | Production | Redis/SQL/配置化；secure defaults | 安全测试 + 压测基准 |
| **M4** | Proven | 多产品接入；Java parity；SLO 证据 | 生产案例 + 质量门禁 |

## 2. 框架 GA 门槛

| 类别 | 最低成熟度 |
| --- | --- |
| 请求生命周期 A1–A7 | M3 |
| 多租户 B1–B3, B5, B9 | M3 |
| 安全 C1–C2, C4–C6, C8, C10 | M3 |
| 韧性 D1–D7 | M3（内存 M3 + Redis/SQL M2 可并行发布） |
| 可观测 E1, E3, E5, E9 | M3 |
| 契约 F1–F3, F7 | M3 |
| 错误 G1, G3, G6 | M3 |
| Bootstrap H1–H2 | M3 |
| 测试 K1–K4 | M3 |

## 3. 极致打磨（M3→M4）清单

### 3.1 性能

| 项 | 目标 |
| --- | --- |
| 链 overhead（内存 store） | p99 < 0.5ms @ 空 Handler — 证据：`tests/architecture/pipeline_benchmark` + `scripts/benchmark-pipeline.*`（K9） |
| 限流检查 | p99 < 1ms（Redis 本地） |
| 零业务分配 | 热路径避免 per-request `String` 克隆 principal 全量 |

### 3.2 安全

| 项 | 目标 |
| --- | --- |
| OWASP API Top 10 映射文档 | 每项对应 EP/阶段 |
| 伪造头 fuzz 测试 | 100% 拒绝 |
| CORS 误配置 lint | prod 禁止 `*` + credentials |

### 3.3 开发者体验

| 项 | 目标 |
| --- | --- |
| 业务集成步骤 | ≤ 3 步（依赖→adapter→layer） |
| `cargo doc` | 每个 EP 有示例 |
| 错误信息 | Problem detail 可行动（缺 token / 缺 scope） |

### 3.4 跨运行时

| 项 | 目标 |
| --- | --- |
| Java Filter 链 | 阶段名 1:1 |
| 共享测试向量 | `tests/vectors/*.json` |

## 4. 能力状态看板（框架仓库 M3 现状）

| 域 | M3 目标数 | 当前实现 | 证据 |
| --- | --- | --- | --- |
| A 生命周期 | 10 | M3 已达成 | pipeline + integration tests |
| B 多租户 | 8 | M3 已达成 | tenant resolver + isolation policies |
| C 安全 | 10 | M3 已达成 | security_vectors + header_fuzz |
| D 韧性 | 7 | M3 已达成（内存 M3；Redis/SQL M2 并行） | store adapters + stress |
| E 可观测 | 7 | M3 已达成 | `traceId` / numeric `code` on Problem+json + OTel feature |
| F 契约 | 6 | M3 已达成 | contract fallback + openapi authority |
| G 错误 | 4 | M3 已达成 | problem_correlation_rules + problem_snapshot |
| H 引导 | 4 | M3 已达成 | bootstrap integration + admin readiness |
| I 存储 | 5 | M2–M3 | redis/sqlx adapters |
| J 验证 | 4 | M3 已达成 | body limit + validation interceptors |
| K 测试 | 12 | M3 已达成 | architecture suite + bootstrap integration + K9 benchmark |

## 5. Definition of Done（单能力）

框架仓库（M3 核心能力）已满足：

- [x] capability matrix JSON 有 entry（`specs/web-framework-capability.matrix.json`）
- [x] docs 13 有 ID 行（`docs/architecture/tech/TECH-13-capability-catalog.md`）
- [x] trait/阶段代码 + 测试（architecture + workspace tests）
- [x] M3 安全/性能证据（`security_vectors`、`pipeline_benchmark`）
- [x] CHANGELOG 条目（`CHANGELOG.md` — 发布时维护）

消费者仓库新增能力时仍按上述清单逐项验收。

## 6. 路线图：极致分三期

| 期 | 范围 | 出口 |
| --- | --- | --- |
| **Phase I** | M3 核心 A/C/G/H + 内存 D + K1–K7 | **框架仓库已达成**；appbase 切换见 [TECH-10-migration-from-appbase.md](./TECH-10-migration-from-appbase.md) |
| **Phase II** | Redis/SQL store M3；E 全项；Java parity | claw-router 接入 |
| **Phase III** | M4 多产品；OTel M2；合规 CI | 对外 GA |

