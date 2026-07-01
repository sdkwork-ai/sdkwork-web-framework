# 生产 Rollout 与多产品采纳证据

> 适用对象：将 `sdkwork-web-framework` 嵌入生产 SaaS / 控制面服务的平台与业务团队。  
> 标准依据：`../sdkwork-specs/QUALITY_GATE_SPEC.md`、`../sdkwork-specs/RELEASE_SPEC.md`、`../sdkwork-specs/DEPLOYMENT_SPEC.md`。

## 1. Rollout 阶段（生产运维上线）

| 阶段 | 目标 | 必做动作 | 退出标准 |
| --- | --- | --- | --- |
| **Pre-flight** | 装配与门禁 | `scripts/verify.ps1` / `verify.sh` 全绿；`deployments/README.md` 清单逐项勾选 | verify 日志归档 |
| **Staging soak** | 依赖与探针 | `/readyz` 覆盖 SQLx + Redis；`SDKWORK_WEB_FRAMEWORK_ENV=prod` 与 `production_defaults()` 一致 | 24h 无 readiness 抖动 |
| **Canary** | 流量渐进 | K8s readiness 摘流 → 单 AZ/单副本 5–10% 流量；监控 429/503/5xx 与 p99 | 错误预算未超阈 |
| **Full rollout** | 全量 | 滚动升级 + graceful shutdown；OTEL trace 采样验证 | SLO 稳定 7 天 |
| **Rollback** | 快速回退 | 保留上一版本镜像与 DB 迁移兼容策略；readyz 失败自动摘流 | RTO < 15m |

### Pre-flight 命令（框架仓库）

```bash
# 自框架仓库根目录
scripts/verify.ps1          # Windows
# scripts/verify.sh       # Unix

# 可选：Live Redis 证据（CI/预发环境设置 SDKWORK_REDIS_TEST_URL）
# cargo test -p sdkwork-web-store-redis --test redis_live -- --ignored
```

### 运行时探针（所有嵌入服务）

- Liveness: `GET /healthz`
- Readiness: `GET /readyz`（必须反映 SQL/Redis 真实状态）
- Metrics: `GET /metrics`（`api_surface` / `backend_layer` 标签见 OBSERVABILITY_SPEC）

详见 [21-operations-runbook.md](./21-operations-runbook.md)。

## 2. 发布证据包（Release Gate）

每次对外交付或消费者 pin 新版本时，归档：

| 证据项 | 路径 / 命令 | 说明 |
| --- | --- | --- |
| Release 证据包 | `node scripts/collect-release-evidence.mjs` | 输出 `target/release-evidence/release-evidence.json` |
| 采纳 JSON 校验 | `node scripts/validate-adoption-evidence.mjs <file>` | QUALITY_GATE 字段校验 |
| Verify 全绿 | `scripts/verify.*` 完整日志 | 含 architecture、contract、PC E2E、clippy |
| Pipeline benchmark | `scripts/benchmark-pipeline.*` | Release p99 预算（K9） |
| CHANGELOG | `CHANGELOG.md` | 与 `component.spec.json` version 对齐 |
| Commit SHA | git rev-parse HEAD | 写入消费者 adoption 记录 |
| OWASP 映射 | [18-owasp-api-top10-mapping.md](./18-owasp-api-top10-mapping.md) | API8 生产装配交叉引用 |
| 装配清单 | [deployments/README.md](../deployments/README.md) | production_defaults + Redis HA |

模板 JSON：`specs/production-adoption.evidence.template.json`（消费者复制并填写）。

## 3. 多产品采纳证据（M4）

框架 **M4** 要求至少 **两个独立产品/服务** 完成生产集成并留下可审计证据（非框架仓库内代码，而在消费者侧归档）。

每个采纳方填写：

1. `productId` — SDKWork 应用或服务标识
2. `frameworkVersion` — 消费的 crate 版本 / git tag
3. `integrationProfile` — `saas` \| `control-plane-standalone`
4. `resolver` — IAM adapter 或 bootstrap lookup 说明
5. `stores` — Redis HA / SQLx 路径
6. `verifyEvidence` — 消费者 CI 中运行的等价门禁链接或日志
7. `productionSince` — ISO 8601 首次生产日期
8. `owner` — 运维/on-call 联系人

示例结构见 `specs/production-adoption.evidence.template.json`。

**框架 pathfinder（本仓库已提交）：** `specs/framework-adoption.evidence.json` 记录 `sdkwork-web-admin-server` 与 `sdkwork-web-framework-pc` 两个可部署面的 verify 证据，用于 schema 与门禁校验；**不替代**外部消费者产品的 M4 采纳签字。

### 采纳验收（单产品）

- [ ] 使用 `production_defaults()` + `validate_production_assembly` 通过
- [ ] backend-api 使用 **ORGANIZATION** `login_scope`（非 TENANT 个人会话）
- [ ] Redis HA store（SaaS）或 control-plane SQLx profile（standalone admin）
- [ ] Problem+json 含 `requestId` / `traceId`
- [ ] 消费者 runbook 链接到框架 [21-operations-runbook.md](./21-operations-runbook.md)

## 4. 安全与合规交叉引用

| 主题 | 文档 |
| --- | --- |
| OWASP API Top 10 | [18-owasp-api-top10-mapping.md](./18-owasp-api-top10-mapping.md) |
| 18 阶段拦截链 | [specs/WEB_FRAMEWORK_STANDARD.md](../specs/WEB_FRAMEWORK_STANDARD.md) |
| 消费者装配 | [23-consumer-integration-template.md](./23-consumer-integration-template.md) |
| Bootstrap / 路由 | [22-bootstrap-and-routing.md](./22-bootstrap-and-routing.md) |

## 5. 故障与回滚 Runbook 摘要

| 信号 | 动作 |
| --- | --- |
| `production assembly is unsafe` panic | 禁止上线；修复 builder/env，见 runbook §8 |
| `/readyz` 503 持续 | 摘流；检查 STORE_URL / REDIS_URL |
| 429 激增 | 检查 Redis 限流 store 与 tier 配置 |
| 501 contract fallback | 预期未实现路由；补 handler 或调整 manifest |

完整表：[21-operations-runbook.md](./21-operations-runbook.md) §8。

## 6. 成熟度说明

- 框架仓库当前矩阵成熟度：**M3**（`specs/web-framework-capability.matrix.json` → `currentMaturity`）
- **M4** 对外 GA 需本文件采纳证据 + K9 release benchmark + 多产品生产案例（Phase III）
- 状态字段 `component.spec.json` → `status: stable` 变更需人工评审（见仓库 `AGENTS.md`）
