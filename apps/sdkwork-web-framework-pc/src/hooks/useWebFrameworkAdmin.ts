import { useCallback, useMemo } from "react";
import type {
  AuditEventRecord,
  ControlNodeRecord,
  CorsPolicyRecord,
  OptionalFeaturesSnapshot,
  RateLimitPolicyRecord,
  RuntimeDefaultsSnapshot,
  SecurityEventRecord,
  TenantRuntimeProfileRecord,
} from "../api/types";
import {
  hasPermission,
  PERM_CONTROL_PLANE,
  PERM_PLATFORM_READ,
  PERM_TENANT_ADMIN,
  readDevAuthClaims,
} from "../devAuth";
import { messages, tabLabels } from "../i18n/messages";
import { readDevAuthToken } from "../sdk/auth/token-provider";
import { getWebFrameworkAdminService } from "../services/web-framework-admin-service";

export type WebFrameworkAdminTab =
  | "defaults"
  | "cors"
  | "rateLimit"
  | "tenant"
  | "nodes"
  | "security"
  | "audit";

const TAB_LABELS: Record<WebFrameworkAdminTab, string> = tabLabels;

function resolveVisibleTabs(): WebFrameworkAdminTab[] {
  const claims = readDevAuthClaims(readDevAuthToken());
  if (
    hasPermission(claims, PERM_CONTROL_PLANE) ||
    hasPermission(claims, PERM_PLATFORM_READ)
  ) {
    return [
      "defaults",
      "cors",
      "rateLimit",
      "tenant",
      "nodes",
      "security",
      "audit",
    ];
  }
  if (hasPermission(claims, PERM_TENANT_ADMIN)) {
    return ["defaults", "cors", "rateLimit", "tenant", "audit"];
  }
  return ["defaults"];
}

export function useWebFrameworkAdmin() {
  const service = useMemo(() => getWebFrameworkAdminService(), []);
  const visibleTabs = useMemo(() => resolveVisibleTabs(), []);

  const loadTab = useCallback(
    async (tab: WebFrameworkAdminTab, environment: string): Promise<unknown> => {
      switch (tab) {
        case "defaults":
          return {
            runtime: await service.runtimeDefaults(),
            optional: await service.optionalFeatures(),
          } satisfies {
            runtime: RuntimeDefaultsSnapshot;
            optional: OptionalFeaturesSnapshot;
          };
        case "cors":
          return service.listCorsPolicies(environment);
        case "rateLimit":
          return service.listRateLimitPolicies(environment);
        case "tenant":
          return service.listTenantProfiles(environment);
        case "nodes":
          return service.listControlNodes(environment);
        case "security":
          return service.listSecurityEvents();
        case "audit":
          return service.listAuditEvents();
      }
    },
    [service],
  );

  const savePayload = useCallback(
    async (tab: WebFrameworkAdminTab, payload: unknown): Promise<void> => {
      switch (tab) {
        case "cors":
          await service.upsertCorsPolicy(payload as CorsPolicyRecord);
          return;
        case "rateLimit":
          await service.upsertRateLimitPolicy(payload as RateLimitPolicyRecord);
          return;
        case "tenant":
          await service.upsertTenantProfile(payload as TenantRuntimeProfileRecord);
          return;
        case "nodes":
          await service.registerControlNode(
            payload as Pick<ControlNodeRecord, "node_id" | "base_url" | "environment"> & {
              region?: string;
            },
          );
          return;
        default:
          throw new Error(messages.saveUnsupportedTab);
      }
    },
    [service],
  );

  const heartbeatNode = useCallback(
    async (nodeId: string) => {
      await service.heartbeatControlNode(nodeId);
    },
    [service],
  );

  const deleteNode = useCallback(
    async (nodeId: string) => {
      await service.deleteControlNode(nodeId);
    },
    [service],
  );

  return {
    visibleTabs,
    tabLabels: TAB_LABELS,
    loadTab,
    savePayload,
    heartbeatNode,
    deleteNode,
  };
}

export type {
  AuditEventRecord,
  ControlNodeRecord,
  CorsPolicyRecord,
  RateLimitPolicyRecord,
  SecurityEventRecord,
  TenantRuntimeProfileRecord,
};
