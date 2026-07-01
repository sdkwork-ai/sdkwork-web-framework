# sdkwork-web-framework-pc

PC 管理控制台（`APP_PC_ARCHITECTURE_SPEC`），用于治理 Web 框架运行时配置：

- CORS / 流控 / 租户运行时 profile
- 分布式控制面节点注册
- 安全事件与审计只读浏览
- 框架默认配置与可选特性快照

## 开发

1. 启动 admin API（共享 SQLx store）：

```bash
# 生产装配需要 HS256 JWT 验签密钥（tenant-bound bootstrap）
export SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET=dev-local-secret
# 可选：bootstrap tenant / kid（默认 bootstrap）
# export SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_TENANT_ID=bootstrap
# export SDKWORK_WEB_FRAMEWORK_JWT_BOOTSTRAP_KEY_ID=bootstrap
# 可选：Redis 就绪探针
# export SDKWORK_WEB_FRAMEWORK_REDIS_URL=redis://127.0.0.1:6379
cargo run -p sdkwork-web-admin-server
```

2. 启动 PC 控制台：

```bash
cd apps/sdkwork-web-framework-pc
npm install
npm run dev
```

Vite 开发服务器将 `/backend` 代理到 `127.0.0.1:3920`。

## 环境

复制 `config/browser/runtime-env.development.example.json` 为本地运行时配置，或设置：

`VITE_SDKWORK_WEB_FRAMEWORK_BACKEND_API_BASE_URL`

本地双令牌开发约定：

- `VITE_SDKWORK_ACCESS_TOKEN`：在 `.env.development.local` 中配置 bootstrap `Access-Token` JWT（租户隔离）。Vite 会把 `VITE_*` 变量内联进客户端 bundle，因此**仅限 dev/E2E**，生产环境绝不可设置。
- `auth_token`：写入浏览器 `sessionStorage['sdkwork.authToken']`（不得使用 `VITE_*_TOKEN` 或 `SDKWORK_AUTH_TOKEN` 环境变量）

生产部署：访问令牌不得烘焙进 bundle，由宿主页面（如 IAM current-session SDK）在运行时通过 `window.__SDKWORK_ADMIN_CREDENTIALS__` 注入，详见 `src/sdk/auth/token-provider.ts`。后端始终通过 18 阶段管道重新校验签名、租户绑定与授权，客户端令牌存储仅是 UX 层，不是安全边界。

## 分布式部署

- 各业务节点连接**同一** `SDKWORK_WEB_FRAMEWORK_STORE_URL` SQLx 库
- 启用 `WebFrameworkOptionalFeatures::production_sqlx()` 与对应 dynamic sources
- 通过本控制台写入 `web_*` 表；节点在请求管道中自动解析 overlay
- `web_control_node` 记录区域/节点 URL，供运维查看心跳与拓扑
