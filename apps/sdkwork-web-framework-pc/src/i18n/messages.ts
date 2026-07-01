/**
 * Lightweight operator-facing message catalog (FRONTEND_CODE_SPEC §3 / BACKEND_UI_SPEC §9).
 *
 * The PC console is an internal admin surface; this module centralizes user-facing
 * strings so they can be audited and, in future, localized without hunting through
 * components. Add new keys here instead of inlining literals.
 */

export const messages = {
  appTitle: "SDKWork Web Framework Console",
  appSubtitle: "分布式运行时治理：CORS / 流控 / 租户配置 / 控制面节点",
  loading: "加载中…",
  environment: "环境",
  refresh: "刷新",
  saveJson: "保存 JSON",
  heartbeatNode: "节点心跳",
  deleteNode: "删除节点",
  jsonPlaceholder: "编辑 upsert JSON",
  nodeIdRequired: "JSON 需包含 node_id",
  saveUnsupportedTab: "当前页签不支持保存",
  empty: "暂无数据",
  forbidden: "没有访问该页签的权限",
  networkError: "网络错误，请检查控制面节点是否可用",
} as const;

export type MessageKey = keyof typeof messages;

export const tabLabels = {
  defaults: "默认配置",
  cors: "CORS",
  rateLimit: "流控策略",
  tenant: "租户运行时",
  nodes: "控制节点",
  security: "安全事件",
  audit: "审计",
} as const;
