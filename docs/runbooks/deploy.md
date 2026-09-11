# Runbook — sdkwork-web-framework 部署 / 升级 / 回滚（中文）

适用环境：`development|test|staging|demo|production`。所有命令默认在本机 WSL 执行，远程主机加 `--host ssh://[user@]host[:port]`。镜像参考：`registry.sdkwork.com/apps/sdkwork-web-framework-standalone:0.1.0`（tag 取自 `sdkwork.app.config.json` → `release.currentVersion`）。

> ℹ️ **模块形态**：本模块为 assembly-only（仅提供装配层 crate，不产出 standalone 服务端二进制），
> 因此镜像构建钩子按“不适用”标注并 fail-closed；若后续落地 standalone gateway 二进制，再按 MODULE_BIN_SPEC.md §4.1 接线。
> 本文档的 §1 安装 / §2 升级 章节在容器镜像产出前不可执行。

## 1. 安装（首次）

```bash
bin/docker-deploy.sh install --environment <development|test|staging|demo|production>
bin/docker-deploy.sh install --environment production --yes   # 生产必须显式 --yes
```

install 会同步 bundle 到 `/opt/deploy/sdkwork-web-framework/bundle`，加载镜像，按实例启动并等待健康门禁（`/healthz`）。

## 2. 升级

staging/demo/production 自动先生成变更前备份（`--skip-backup` 可跳过，会记录证据）：

```bash
bin/docker-image.sh build
bin/docker-deploy.sh upgrade --environment staging --image-tag 0.1.0
```

## 3. 验证（发布门禁）

```bash
bin/docker-deploy.sh status --environment staging
bin/doctor.sh --environment staging          # 聚合诊断（9 项检查）
```

## 4. 回滚

```bash
bin/docker-deploy.sh rollback --environment staging                  # 台账上一个成功版本
bin/docker-deploy.sh rollback --environment staging --to 0.1.0       # 指定版本
```

回滚由管理端口 `/healthz` 门禁把关；失败自动回退并写入 `release-state/<env>/ledger.jsonl`。迁移是前向的：跨不兼容 schema 只能走数据恢复（backup-restore.md）。

## 5. 下线

```bash
bin/docker-deploy.sh down --environment staging
bin/docker-deploy.sh stop    --environment staging   # 停止（保留容器与卷，不重打包）
bin/docker-deploy.sh start   --environment staging   # 启动已停止的栈（先起嵌入式依赖）
bin/docker-deploy.sh restart --environment staging   # 只重启应用实例（依赖不中断）
bin/docker-deploy.sh down --environment staging --purge --yes
```

## 6. 接线前置条件（实施清单）

1. 本模块暂无 standalone 服务端二进制（assembly-only）。若产品决定以容器交付，需先落地 standalone gateway crate（参照同族模块的 `sdkwork-api-<module>-standalone-gateway`）。
2. 二进制落地后：实现 `bin/lib/module.sh` → `sdkwork_image_build`，并按 OPERATIONS_SPEC.md §1.2 / DOCKER_SPEC.md §4 落地 `deployments/docker/bundle/`。
3. 验收：`node ../sdkwork-specs/tools/check-operations-conformance.mjs --root .` 全绿。

<!-- generated: scaffold-module-runbooks.mjs -->
