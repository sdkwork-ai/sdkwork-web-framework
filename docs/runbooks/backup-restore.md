# Runbook — sdkwork-web-framework 备份与恢复（中文）

## 1. 备份

```bash
bin/backup.sh create --environment production            # 配置 + 数据库 + 卷，含 sha256
bin/backup.sh list   --environment production
bin/backup.sh verify --environment production            # 校验最新集合
```

备份集位于目标机 `/opt/deploy/sdkwork-web-framework/backups/`。RPO：生产每日 + 每次升级前；RTO：生产 4 小时内完成恢复。

## 2. 恢复（破坏性，需 --yes）

```bash
bin/backup.sh restore --environment production --set <集合名> --yes
bin/docker-deploy.sh install --environment production     # 恢复后重新拉起
```

## 3. 演练

每季度在临时环境真实恢复一次（不是只跑 verify）。

## 6. 接线前置条件（实施清单）

1. 本模块暂无 standalone 服务端二进制（assembly-only）。若产品决定以容器交付，需先落地 standalone gateway crate（参照同族模块的 `sdkwork-api-<module>-standalone-gateway`）。
2. 二进制落地后：实现 `bin/lib/module.sh` → `sdkwork_image_build`，并按 OPERATIONS_SPEC.md §1.2 / DOCKER_SPEC.md §4 落地 `deployments/docker/bundle/`。
3. 验收：`node ../sdkwork-specs/tools/check-operations-conformance.mjs --root .` 全绿。

<!-- generated: scaffold-module-runbooks.mjs -->
