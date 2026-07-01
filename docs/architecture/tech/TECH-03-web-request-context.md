> Migrated from `docs/architecture/tech/TECH-03-web-request-context.md` on 2026-06-24.
> Owner: SDKWork maintainers

# WebRequestContext 规范（极致版）

> 权威：`specs/WEB_FRAMEWORK_STANDARD.md` · 对齐 `sdkwork-specs/API_SPEC.md` §10  
> 实现 crate：`sdkwork-web-core`（类型）+ `sdkwork-web-axum`（自动注入）

## 1. 设计目标

| 目标 | 含义 |
| --- | --- |
| **一次解析** | 全请求生命周期只有一个 `WebRequestContext` 实例 |
| **自动注入** | 所有 API Handler **无需手写**；签名声明即注入 |
| **租户 + 应用持有** | `tenant_id`、`organization_id`、`app_id` 等自 **已验证凭证** 解析，非客户端参数 |
| **类型安全** | Service 层接收 `&WebRequestContext` 或 `TenantAppContext` 视图，不接触 Header |
| **跨语言一致** | Rust / Java / OpenAPI 字段名与语义 1:1 |

---

## 2. 类型总览

```text
WebRequestContext                          # 每个 HTTP 请求一个（Clone）
├── request_id: ServerRequestId            # 关联、日志、Problem 响应
├── trace_id: Option<String>               # W3C trace id（自 traceparent 解析）
├── api_surface: WebApiSurface
├── auth_mode: WebAuthMode
├── transport: WebTransportFacts           # path/method/凭证存在性（只读快照）
├── locale: Option<WebLocale>
├── client_kind: Option<WebClientKind>       # UA 推断；JSON 字段 clientKind
├── operation: Option<WebOperationBinding> # 可选：绑定 operationId
└── principal: Option<WebRequestPrincipal> # 见 §3.3 / WEB_FRAMEWORK_STANDARD P5

WebRequestPrincipal                        # 已解析的 SaaS 主体（Clone）
├── tenancy: WebTenancyContext             # ★ 租户 / 组织
├── app: WebAppContext                     # ★ 应用 / 环境 / 部署
├── subject: WebSubjectContext             # 用户 / 会话 / 主体类型
├── auth: WebAuthContext                   # 认证级别 / API Key / OAuth Bearer
└── scopes: WebScopeContext                # data_scope / permission_scope
```

---

## 3. WebRequestContext — 字段定义

### 3.1 完整结构（Rust 参考）

```rust
/// 框架边界注入；Handler/Service 唯一允许的请求上下文类型。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebRequestContext {
    /// 服务端权威请求 ID（UUID v4）。覆盖客户端 X-Request-Id。
    pub request_id: ServerRequestId,

    /// W3C trace id，自 inbound `traceparent` 解析；用于 Problem+json `traceId`。
    pub trace_id: Option<String>,

    /// 接口面分类结果。
    pub api_surface: WebApiSurface,

    /// 本路径要求的认证模式（由面分类 + manifest 决定）。
    pub auth_mode: WebAuthMode,

    /// 传输层事实（不可变快照）。
    pub transport: WebTransportFacts,

    /// 已认证主体。受保护路由 MUST 非空；公共路由语义见 P5（租户隔离 principal 与 credential-entry 例外）。
    pub principal: Option<WebRequestPrincipal>,

    /// Accept-Language 解析结果。
    pub locale: Option<WebLocale>,

    /// 客户端形态推断（UA / 自定义头，可选）。序列化名 `clientKind`。
    pub client_kind: Option<WebClientKind>,

    /// 与路由 manifest 绑定的 operation（若已解析）。
    pub operation: Option<WebOperationBinding>,
}
```

### 3.2 WebTransportFacts

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebTransportFacts {
    pub path: String,              // 实际路径，如 /app/v3/api/auth/sessions
    pub method: String,            // GET, POST, ...
    pub auth_token_present: bool,  // Authorization 非空
    pub access_token_present: bool,// Access-Token 非空
    pub api_key_present: bool,     // X-API-Key（或契约声明别名）非空
}
```

### 3.3 principal 与 Public 路径（对齐 WEB_FRAMEWORK_STANDARD P5）

| 场景 | `auth_mode` | `principal` |
| --- | --- | --- |
| 受保护业务路由 | `DualToken` / `ApiKey` / `OAuth` | MUST 非空（已验证主体） |
| 公共路由（租户隔离） | `Public` | MUST 非空：自 `Access-Token` JWT 解析的租户隔离 principal（含 `token_version`） |
| 凭证入口路由（`forbidCredentialHeaders`） | `Public` | MUST 为 `None`：接受 bootstrap `Access-Token` JWT 但不建立会话 principal |

分号 claim-string 形态的 `Access-Token` MUST 被拒绝。

### 3.4 WebOperationBinding（可选）

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebOperationBinding {
    pub operation_id: String,      // 如 sessions.create
    pub route_template: String,    // 日志用模板，如 /app/v3/api/auth/sessions
    pub rate_limit_tier: Option<RateLimitTier>,
    pub idempotent: bool,
}
```

### 3.5 枚举

```rust
pub enum WebApiSurface {
    OpenApi,
    AppApi,
    BackendApi,
    GatewayApi,   // 如 /v1，策略可独立
    Unknown,
}

pub enum WebAuthMode {
    Public,      // 无需 principal
    ApiKey,      // open-api（X-Api-Key）
    OAuth,       // open-api（Authorization: Bearer）
    DualToken,   // app-api / backend-api
}

pub enum WebClientKind {
    Browser,
    Mobile,
    Desktop,
    Server,
    Unknown,
}
```

---

## 4. WebRequestPrincipal — 租户与应用（核心）

### 4.1 分组结构

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebRequestPrincipal {
    pub tenancy: WebTenancyContext,
    pub app: WebAppContext,
    pub subject: WebSubjectContext,
    pub auth: WebAuthContext,
    pub scopes: WebScopeContext,
}
```

### 4.2 WebTenancyContext（★ 租户）

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebTenancyContext {
    /// 当前租户 ID。非空。平台级操作为 "0"（须显式授权）。
    pub tenant_id: String,

    /// 组织 ID。租户级登录为 None 或 "0"。
    pub organization_id: Option<String>,

    /// 与 organization_id 一致性强绑定。
    pub login_scope: WebLoginScope,
}

pub enum WebLoginScope {
    Tenant,        // organization_id 为空或 "0"
    Organization,  // organization_id 非空且非 "0"
}
```

| 字段 | 来源 | 禁止 |
| --- | --- | --- |
| `tenant_id` | 验签后的 token / API Key 记录 / OAuth bearer 记录 | Query/Body/Header `tenantId` |
| `organization_id` | 同上 | 客户端投影头 |
| `login_scope` | token claim `TENANT`/`ORGANIZATION` | 与 org_id 矛盾的组合 |

### 4.3 WebAppContext（★ 应用）

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebAppContext {
    /// PlusApp / 注册应用 ID。所有 app-api 必须非空。
    pub app_id: String,

    /// 运行环境。
    pub environment: WebEnvironment,

    /// 部署模式。
    pub deployment_mode: WebDeploymentMode,

    /// 可选：工作区隔离（组合运行时）。
    pub workspace_id: Option<String>,

    /// 可选：依赖组合实例 ID。
    pub composition_id: Option<String>,
}

pub enum WebEnvironment { Dev, Test, Prod }
pub enum WebDeploymentMode { Saas, Private, Local }
```

| 字段 | 语义 | app-api 要求 |
| --- | --- | --- |
| `app_id` | 当前请求归属的应用 | **MUST** 非空 |
| `environment` | 配置剖面 | MUST |
| `deployment_mode` | SaaS/私有化/本地 | MUST |
| `workspace_id` | 多工作区产品可选 | MAY |
| `composition_id` | SDK 组合运行时可选 | MAY |

### 4.4 WebSubjectContext

```rust
pub struct WebSubjectContext {
    pub user_id: String,
    pub session_id: Option<String>,
    pub subject_type: WebSubjectType,  // User | Service | ApiKey | System
}

pub enum WebSubjectType {
    User,
    Service,
    ApiKey,
    System,
}
```

### 4.5 WebAuthContext

```rust
pub struct WebAuthContext {
    pub auth_level: WebAuthLevel,
    pub api_key_id: Option<String>,  // open-api 时填充
}

pub enum WebAuthLevel {
    Anonymous,
    Password,
    Mfa,
    System,
    ApiKey,
}
```

### 4.6 WebScopeContext

```rust
pub struct WebScopeContext {
    /// 数据域标签（不透明，业务解释）。
    pub data_scope: Vec<String>,
    /// 权限/operation grant 标签。
    pub permission_scope: Vec<String>,
}
```

---

## 5. 便捷访问 API（框架提供）

Handler/Service **MUST** 使用下列方法，**禁止** 直接解构深层字段后传递零散 `String`（除非性能关键路径且已文档化）。

```rust
impl WebRequestContext {
  // ── 关联 ID ──
    pub fn request_id(&self) -> &str;
    pub fn request_id_uuid(&self) -> &Uuid;

  // ── 面与认证 ──
    pub fn api_surface(&self) -> WebApiSurface;
    pub fn auth_mode(&self) -> WebAuthMode;
    pub fn is_public(&self) -> bool;

  // ── ★ 租户（快捷）──
    pub fn tenant_id(&self) -> Option<&str>;
    pub fn organization_id(&self) -> Option<&str>;
    pub fn login_scope(&self) -> Option<WebLoginScope>;

  // ── ★ 应用（快捷）──
    pub fn app_id(&self) -> Option<&str>;
    pub fn environment(&self) -> Option<WebEnvironment>;
    pub fn deployment_mode(&self) -> Option<WebDeploymentMode>;
    pub fn workspace_id(&self) -> Option<&str>;

  // ── 主体 ──
    pub fn user_id(&self) -> Option<&str>;
    pub fn session_id(&self) -> Option<&str>;
    pub fn principal(&self) -> Option<&WebRequestPrincipal>;

  // ── 强制（protected Handler）──
    pub fn require_principal(&self) -> Result<&WebRequestPrincipal, WebFrameworkError>;
    pub fn require_tenant_id(&self) -> Result<&str, WebFrameworkError>;
    pub fn require_app_id(&self) -> Result<&str, WebFrameworkError>;

  // ── 权限 ──
    pub fn has_permission(&self, grant: &str) -> bool;
    pub fn has_data_scope(&self, tag: &str) -> bool;

  // ── 视图 ──
    pub fn tenancy(&self) -> Option<&WebTenancyContext>;
    pub fn app_context(&self) -> Option<&WebAppContext>;
}
```

### 5.1 TenantAppContext 视图（Service 层推荐）

```rust
/// Service/Repository 层推荐的轻量视图，仅含租户+应用+主体 ID。
#[derive(Clone, Debug)]
pub struct TenantAppContext<'a> {
    pub request_id: &'a str,
    pub tenant_id: &'a str,
    pub organization_id: Option<&'a str>,
    pub app_id: &'a str,
    pub user_id: &'a str,
    pub environment: WebEnvironment,
    pub deployment_mode: WebDeploymentMode,
}

impl<'a> From<&'a WebRequestContext> for TenantAppContext<'a> { ... }
```

Repository **MUST** 以 `TenantAppContext` 或 `&WebRequestContext` 接收租户条件，**MUST NOT** 单独接收裸 `tenant_id: String` 参数而无上下文来源。

---

## 6. 自动注入（所有 API 方法）

### 6.1 规则（强制）

| # | 规则 |
| --- | --- |
| I1 | 凡挂载在 `app-api` / `backend-api` / `open-api` 前缀下的 Router **MUST** 应用 `with_web_request_context`（或等价 Layer） |
| I2 | 仅 `/healthz`、`/metrics`、静态资源 **MAY** 使用 `with_server_request_identity` 轻量层 |
| I3 | 每个业务 Handler **MUST** 将 `WebRequestContext` 列为参数（或通过 `RequirePrincipal` 等派生 extractor） |
| I4 | `ContextInjection` 阶段 **MUST** 将 `WebRequestContext` 写入 `Extensions` |
| I5 | protected Handler **MUST NOT** 在 `principal == None` 时执行业务逻辑 |
| I6 | OpenAPI 每个受保护 operation **MUST** 声明 `x-sdkwork-request-context: WebRequestContext` |

### 6.2 Rust — 自动注入机制

#### 层 1：Middleware（全员必经）

```rust
// sdkwork-web-axum
router.layer(web_framework.request_context_layer())
```

`ContextInjection` 阶段调用：

```rust
fn inject_web_request_context(request: &mut Request, ctx: WebRequestContext) {
    request.extensions_mut().insert(ctx.request_id.clone());
    request.extensions_mut().insert(ctx);  // ★ 主上下文
    for injector in &runtime.domain_injectors {
        injector.inject(request, &ctx);
    }
}
```

#### 层 2：Extractor（Handler 自动注入）

`WebRequestContext` 实现 `FromRequestParts`：

```rust
impl<S> FromRequestParts<S> for WebRequestContext
where
    S: Send + Sync,
{
    type Rejection = WebFrameworkError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<WebRequestContext>().cloned().ok_or(
            WebFrameworkError::context_not_injected()
        )
    }
}
```

**Handler 写法（标准）**：

```rust
async fn sessions_create(
    ctx: WebRequestContext,           // ★ 自动注入，无需 Extension 包装
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<SdkWorkApiResponse<SessionDto>>, WebFrameworkError> {
    let tenant_id = ctx.require_tenant_id()?;
    let app_id = ctx.require_app_id()?;
    state.auth_service.create_session(&ctx, body).await
}
```

#### 层 3：派生 Extractor（按认证要求）

```rust
/// protected 路由：principal 必须存在
pub struct RequirePrincipal(pub WebRequestPrincipal);
impl FromRequestParts<S> for RequirePrincipal { ... }

/// 仅需租户+应用视图
pub struct TenantApp(pub TenantAppContext<'static>); // 或生命周期绑定技巧
```

#### 层 4：Router 注册约束（manifest 驱动 lint 目标）

```rust
// manifest.rs — 每个路由声明 auth
HttpRoute {
    method: HttpMethod::Post,
    path: "/app/v3/api/auth/sessions",
    operation_id: "sessions.create",
    auth: RouteAuth::Public,           // 仍注入 ctx，principal 为 None
    ...
}
HttpRoute {
    ...
    auth: RouteAuth::DualToken,        // principal MUST 非空
    ...
}
```

### 6.3 Java — 自动注入（对齐）

```java
// Filter 链等价于 with_web_request_context
@RequestAttribute(WebRequestContext.ATTR_NAME)
WebRequestContext ctx;

// 或方法参数解析器 HandlerMethodArgumentResolver
public SdkWorkApiResponse<SessionDto> create(
    @InjectWebRequestContext WebRequestContext ctx,
    @RequestBody CreateSessionRequest body
) { ... }
```

### 6.4 OpenAPI / SDK

```yaml
paths:
  /app/v3/api/auth/sessions/current:
    get:
      operationId: sessions.current.retrieve
      x-sdkwork-request-context: WebRequestContext
      x-sdkwork-api-surface: app-api
```

生成 SDK **不得** 添加 `tenantId` / `appId` 参数；租户与应用来自 Token，由服务端注入上下文。

---

## 7. 生命周期与不可变性

```text
Request 进入
  → Interceptor 链 mut WebCallState（内部构建 principal）
  → ContextInjection：构建最终 WebRequestContext（Clone 存入 Extensions）
  → Handler 接收 &WebRequestContext（只读）
  → Service 传递 &WebRequestContext 或 TenantAppContext
  → Repository 使用 ctx.tenant_id() 等构建 SQL 条件
Response 返回（request_id 写入响应头）
```

| 阶段 | 谁可写 principal |
| --- | --- |
| RequestContextResolution 之前 | 无 |
| Resolver | 产出 WebRequestPrincipal |
| Handler 及之后 | **禁止修改**（`WebRequestContext` 无 `mut` setter） |

---

## 8. Public vs Protected 上下文形态

| 路径类型 | auth_mode | principal | Handler 要求 |
| --- | --- | --- | --- |
| 公共登录/注册 | `Public` | `None` | `ctx: WebRequestContext`；不得调用 `require_principal` |
| app-api 受保护 | `DualToken` | `Some(...)` | `require_tenant_id` + `require_app_id` |
| backend-api | `DualToken` | `Some(...)` | 同上 + 更高 `auth_level` |
| open-api（api-key） | `ApiKey` | `Some(...)` | `require_tenant_id`；`app_id` 来自 key 记录 |
| open-api（oauth） | `OAuth` | `Some(...)` | `require_tenant_id`；`app_id` 来自 token/session 记录 |
| open-api（flexible） | `ApiKey` 或 `OAuth`（Header 驱动） | `Some(...)` | 同上；默认 API Key 优先于 OAuth Bearer |

**注意**：公共路径 **仍然注入** `WebRequestContext`（含 `request_id`、surface），仅 `principal` 为空。

---

## 9. JSON Schema（OpenAPI components）

见 [specs/web-request-context.schema.json](../specs/web-request-context.schema.json)。

核心嵌套：

```json
{
  "WebRequestContext": {
    "required": ["requestId", "apiSurface", "authMode", "transport"],
    "properties": {
      "requestId": { "type": "string", "format": "uuid" },
      "apiSurface": { "enum": ["openApi", "appApi", "backendApi", "gatewayApi", "unknown"] },
      "authMode": { "enum": ["public", "apiKey", "oauth", "dualToken"] },
      "principal": { "$ref": "#/components/schemas/WebRequestPrincipal" }
    }
  },
  "WebRequestPrincipal": {
    "required": ["tenancy", "app", "subject", "auth", "scopes"],
    "properties": {
      "tenancy": { "$ref": "#/components/schemas/WebTenancyContext" },
      "app": { "$ref": "#/components/schemas/WebAppContext" }
    }
  }
}
```

JSON 对外命名 **camelCase**（`tenantId`, `appId`）；Rust 内部 **snake_case**（serde `rename_all = "camelCase"`）。

---

## 10. 禁止事项（lint / 代码评审）

| 禁止 | 替代 |
| --- | --- |
| Handler 参数无 `WebRequestContext`（业务 API） | 必须添加 |
| `headers.get("Authorization")` | `ctx` + resolver 已完成 |
| `headers.get("x-sdkwork-tenant-id")` | `ctx.tenant_id()` |
| 从 `Json<T>` 读取 `tenant_id` 作为当前租户 | `ctx.require_tenant_id()` |
| 手动 `extensions.insert(WebRequestContext)` | 仅框架 ContextInjection |
| Service 方法签名无上下文来源 | `fn(&self, ctx: &WebRequestContext, ...)` |

---

## 11. 与 API_SPEC §10 映射

| API_SPEC `AppRequestPrincipal` | WebRequestContext 位置 |
| --- | --- |
| tenant_id | `principal.tenancy.tenant_id` |
| organization_id | `principal.tenancy.organization_id` |
| login_scope | `principal.tenancy.login_scope` |
| user_id | `principal.subject.user_id` |
| session_id | `principal.subject.session_id` |
| app_id | `principal.app.app_id` |
| environment | `principal.app.environment` |
| deployment_mode | `principal.app.deployment_mode` |
| auth_level | `principal.auth.auth_level` |
| data_scope | `principal.scopes.data_scope` |
| permission_scope | `principal.scopes.permission_scope` |
| api_key_id | `principal.auth.api_key_id` |
| subject_type | `principal.subject.subject_type` |

迁移别名：`pub type AppRequestContext = WebRequestContext;`

---

## 12. 验收清单（极致）

- [x] 所有 app/backend/open API Handler 签名含 `WebRequestContext` 或派生 extractor（框架 control-plane：`handler_static_rules`）
- [x] Router 100% 挂载 `with_web_request_context`（除白名单基础设施路径；见 `WEB_FRAMEWORK_STANDARD` I1）
- [x] `FromRequestParts` 实现；缺失注入返回 500 `context_not_injected`（`WebFrameworkRejection` + axum integration 测试）
- [x] `WebTenancyContext` + `WebAppContext` 分组；快捷方法 `tenant_id()` / `app_id()`
- [x] `TenantAppContext` 视图供 Repository 使用
- [x] Public 租户隔离路由 principal 自 Access-Token 解析；credential-entry 路由 principal 为 None（`security_vectors`）
- [x] OpenAPI schema + `x-sdkwork-request-context` 全覆盖（`openapi_authority` / `openapi_context_selectors`）
- [x] 静态扫描：route crate 无 raw Authorization 解析（`handler_static_rules`）
- [x] 集成测试：protected 路由无 token → 401，有 token → ctx 含 tenant+app（`security_vectors` / axum integration）
- [x] Problem+json 错误响应含 numeric `code` 与 `traceId`（`problem_correlation_rules` / admin_api / pipeline integration）

## 13. 相关文档

- [TECH-04-pipeline-interceptor-design.md](./TECH-04-pipeline-interceptor-design.md) — ContextInjection 阶段
- [TECH-15-extension-points-registry.md](./TECH-15-extension-points-registry.md) — EP-01, EP-08
- [specs/web-request-context.schema.json](../specs/web-request-context.schema.json)

