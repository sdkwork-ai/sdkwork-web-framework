> Migrated from `docs/architecture/tech/TECH-21-operations-runbook.md` on 2026-06-24.
> Owner: SDKWork maintainers

# 生产运维手册（Operations Runbook）

> 适用对象：`sdkwork-web-admin-server` 独立控制面二进制，以及嵌入 `WebFramework` 的业务 API 服务。  
> 标准依据：`../sdkwork-specs/OBSERVABILITY_SPEC.md`、`../sdkwork-specs/DEPLOYMENT_SPEC.md`、`deployments/README.md`。

## 1. 二进制与端点

| 组件 | 说明 |
| --- | --- |
| `sdkwork-web-admin-server` | 可选独立 admin/control-plane HTTP 服务 |
| `/healthz` | Liveness — 进程存活 |
| `/readyz` | Readiness — 依赖探针（SQLx / Redis 等 `ReadinessCheck`） |
| `/metrics` | Prometheus 指标（pipeline stage、HTTP labels） |

业务服务通过 `WebFramework::mount_service_routes` 挂载相同运维端点。

## 2. 环境变量

完整模板：`configs/admin-server.env.example`。框架词汇由 `WebFrameworkEnv::from_process_env()` 解析（catalog H5）。

| 变量 | 必需 | 说明 |
| --- | --- | --- |
| `SDKWORK_WEB_FRAMEWORK_ENV` | 生产推荐 `prod` | 触发 `validate_production_assembly` |
| `SDKWORK_WEB_FRAMEWORK_ADMIN_BIND` | 否 | 默认 `127.0.0.1:3920` |
| `SDKWORK_WEB_FRAMEWORK_STORE_URL` | 是 | SQLx 连接串（`web_*` 表） |
| `SDKWORK_WEB_FRAMEWORK_STORE_POOL_SIZE` | 否 | SQLx 连接池大小（默认 `8`） |
| `SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET` | admin-server 是 | 控制面 bootstrap JWT 签名 |
| `SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID` | 否 | 默认 `bootstrap` |
| `SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_KEY_ID` | 否 | 默认 `bootstrap` |
| `SDKWORK_WEB_FRAMEWORK_REDIS_URL` | 生产 SaaS 推荐 | HA Redis；限流/幂等/并发准入 |
| `OTEL_SERVICE_NAME` | 否 | OpenTelemetry 服务名 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | 否 | OTLP HTTP 导出端点 |
| `RUST_LOG` | 否 | tracing 过滤器 |

## 3. 启动流程

```bash
# 1. 准备 env（见 configs/admin-server.env.example）
# 2. 构建
cargo build --release -p sdkwork-web-admin-server
# 3. 运行
./target/release/sdkwork-web-admin-server
```

进程启动时调用 `init_tracing_from_env()`：若设置 `OTEL_EXPORTER_OTLP_ENDPOINT` 且二进制启用 `otel` feature，则导出分布式 trace；否则结构化本地日志（凭证头脱敏）。

`WebFrameworkBuilder::production_defaults()` + `enable_admin_api` 自动：

- 装配 production 超时与 graceful shutdown 窗口
- 从 `ROUTES` 推导 `route_manifest` 与 contract fallback（501/404 Problem+json）
- 执行 `validate_production_assembly`（禁止 dev resolver、非 HA store、不安全 CORS 等）

## 4. 健康检查与就绪

**Liveness** — `GET /healthz` 返回 200 表示进程正常。

**Readiness** — `GET /readyz`：

- 未配置 `ReadinessCheck` 时返回 503（生产 SaaS builder 拒绝缺少 probe 的装配）
- 配置 `SqliteReadinessCheck` + 可选 `RedisReadinessCheck` 后，依赖可用才返回 200

Kubernetes 示例：

```yaml
livenessProbe:
  httpGet:
    path: /healthz
    port: 3920
  initialDelaySeconds: 5
readinessProbe:
  httpGet:
    path: /readyz
    port: 3920
  initialDelaySeconds: 10
  periodSeconds: 5
```

## 5. 指标与关联

- Prometheus scrape：`GET /metrics`
- Problem+json 响应含 numeric `code` 与服务端 `traceId`（`X-SdkWork-Trace-Id`；W3C `traceparent` 传播）
- 日志字段与 `WebRequestContext` 对齐；禁止向客户端泄露栈跟踪

## 6. 优雅关闭

`WebFramework::run` 使用 `serve_with_graceful_shutdown`：

- Unix：SIGTERM + Ctrl+C
- Windows：Ctrl+C
- `shutdown_grace_period`（production 默认）为连接 drain 的最大等待时间；收到 SIGTERM/Ctrl+C 后立即停止接受新连接，并在 drain 期间异步执行 lifecycle `on_shutdown` 清理依赖

滚动升级：先 `/readyz` 失败摘流，再 SIGTERM，等待 grace period 后退出。

## 7. 生产 SaaS 存储要求

| 能力 | 开发 | 生产 SaaS |
| --- | --- | --- |
| 限流 | Memory / SQLx | **Redis HA**（`is_distributed_ha() = true`） |
| 幂等 | Memory / SQLx | **Redis HA** |
| 并发准入 | Memory | **Redis HA**（多副本） |
| JWT 验证 | Env bootstrap lookup | **IAM adapter** `TenantSigningKeyLookup` + `JwtSessionRevocationChecker` |

详见 [TECH-10-migration-from-appbase.md](./TECH-10-migration-from-appbase.md)。

## 8. 故障排查

| 现象 | 可能原因 | 动作 |
| --- | --- | --- |
| 启动 panic `production assembly is unsafe` | dev resolver / 非 HA store / 缺 readiness | 检查 builder 与 env；见 `validate_production_assembly` 消息 |
| `/readyz` 503 | DB 或 Redis 不可用 | 检查 `SDKWORK_WEB_FRAMEWORK_STORE_URL` / `REDIS_URL` |
| 501 Problem `not-implemented` | manifest 路由未挂载 handler | 预期 contract fallback；实现 handler 或调整 manifest |
| 401/403 无 traceId | 客户端未传 traceparent | 正常；服务端仍生成 `traceId` |
| OTEL 无 span | 未设 endpoint 或未启用 otel feature | 检查 env 与 binary features |

## 9. 发布门禁

发布前必须：

1. `scripts/verify.ps1` 或 `scripts/verify.sh` 全绿
2. 记录 `CHANGELOG.md` 条目
3. 附 verify 日志与 benchmark 输出（`QUALITY_GATE_SPEC` 证据）
4. 完成 [TECH-24-production-rollout-and-adoption.md](./TECH-24-production-rollout-and-adoption.md) Pre-flight 清单；M4 需归档多产品采纳 JSON

## 10. 相关文档

- [deployments/README.md](../deployments/README.md) — 装配清单
- [configs/admin-server.env.example](../configs/admin-server.env.example) — 环境模板
- [TECH-16-maturity-model.md](./TECH-16-maturity-model.md) — M3 GA 门槛

