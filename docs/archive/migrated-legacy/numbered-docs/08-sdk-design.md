# SDK 设计

> 前置阅读：[00-framework-foundation.md](./00-framework-foundation.md)

## 1. 框架 SDK 边界

| 提供 | 不提供 |
| --- | --- |
| 类型/bootstrap SDK（`WebRequestContext` 词汇、常量、Interceptor 列表） | 业务 app-api/backend-api 客户端 |
| `component.spec.json` 声明标准链 | IAM/commerce 等 API authority |
| 物化 **types** OpenAPI（无 path 或仅 schema） | 替代产品 `sdkgen` |

**业务 SDK 仍在各业务仓库 `sdks/`**（如 `sdkwork-iam-app-sdk`）。

## 2. 可选 SDK 家族

```text
sdks/sdkwork-web-framework-types-sdk/
  specs/component.spec.json
  openapi/web-framework-types.openapi.yaml   # components only / 空 paths
  typescript/ java/ ...                      # sdkgen 生成
```

用途：

- 前端/文档站展示 `WebRequestPrincipal` schema
- 跨语言 bootstrap 常量（接口面前缀、标准头名）
- **不是** HTTP 业务调用客户端

## 3. component.spec.json（框架仓库根）

```json
{
  "component": {
    "name": "sdkwork-web-framework",
    "domain": "platform",
    "capability": "web-framework",
    "type": "rust-workspace"
  },
  "contracts": {
    "requestContextFramework": {
      "contextType": "WebRequestContext",
      "standardInterceptors": ["request_identity", "..."]
    },
    "apiSurfaces": ["open-api", "app-api", "backend-api"],
    "extensionTraits": [
      "WebRequestContextResolver",
      "AuthorizationPolicy",
      "TenantIsolationPolicy",
      "DomainContextInjector",
      "ApiKeyLookupService",
      "OAuthTokenLookupService",
      "OpenApiCredentialSchemeDetector"
    ]
  }
}
```

## 4. 业务 SDK 如何声明框架

业务 `component.spec.json` **引用**框架标准，不 fork：

```json
{
  "contracts": {
    "requestContextFramework": {
      "contextType": "WebRequestContext",
      "frameworkPackage": "sdkwork-web-core",
      "frameworkVersion": "^0.1.0"
    }
  }
}
```

OpenAPI operation 扩展：

```yaml
x-sdkwork-request-context: WebRequestContext
```

## 5. Rust 依赖

```toml
# 业务 Cargo.toml
sdkwork-web-core = { path = "../sdkwork-web-framework/crates/sdkwork-web-core" }
sdkwork-web-axum = { path = "../sdkwork-web-framework/crates/sdkwork-web-axum" }
```

## 6. 验证

```bash
sdkgen verify --authority sdkwork-web-framework-types  # 若启用
cargo test --workspace
```

## 7. 禁止

- 在框架 SDK 中生成 `/app/v3/api/iam/**` 等路径
- hand-edit 生成 transport 代码
