import type {
  AuditEventRecord,
  ControlNodeRecord,
  CorsPolicyRecord,
  OptionalFeaturesSnapshot,
  RateLimitPolicyRecord,
  RuntimeDefaultsSnapshot,
  SecurityEventRecord,
  TenantRuntimeProfileRecord,
} from "../../api/types";
import { webFrameworkAdminOperations } from "./operations";
import { createBackendSdkTransport, query, type BackendSdkTransport } from "./transport";

export type WebFrameworkAdminBackendSdk = ReturnType<typeof createWebFrameworkAdminBackendSdk>;

/** Backend SDK facade for framework control-plane `/backend/v3/api/web-framework` operations. */
export function createWebFrameworkAdminBackendSdk(
  baseUrl: string,
  transport: BackendSdkTransport = createBackendSdkTransport(baseUrl),
) {
  const ops = webFrameworkAdminOperations;
  return {
    listCorsPolicies: (environment?: string, limit = 50) =>
      transport.get<CorsPolicyRecord[]>(
        `${ops.corsPolicies.list}${query({ environment, limit: String(limit) })}`,
      ),
    upsertCorsPolicy: (payload: CorsPolicyRecord) =>
      transport.put<CorsPolicyRecord>(ops.corsPolicies.upsert, payload),
    listRateLimitPolicies: (environment?: string, limit = 50) =>
      transport.get<RateLimitPolicyRecord[]>(
        `${ops.rateLimitPolicies.list}${query({ environment, limit: String(limit) })}`,
      ),
    upsertRateLimitPolicy: (payload: RateLimitPolicyRecord) =>
      transport.put<RateLimitPolicyRecord>(ops.rateLimitPolicies.upsert, payload),
    listTenantProfiles: (environment?: string, limit = 50) =>
      transport.get<TenantRuntimeProfileRecord[]>(
        `${ops.tenantRuntimeProfiles.list}${query({ environment, limit: String(limit) })}`,
      ),
    upsertTenantProfile: (payload: TenantRuntimeProfileRecord) =>
      transport.put<TenantRuntimeProfileRecord>(ops.tenantRuntimeProfiles.upsert, payload),
    listControlNodes: (environment?: string, limit = 50) =>
      transport.get<ControlNodeRecord[]>(
        `${ops.controlNodes.list}${query({ environment, limit: String(limit) })}`,
      ),
    registerControlNode: (
      payload: Pick<ControlNodeRecord, "node_id" | "base_url" | "environment"> & {
        region?: string;
      },
    ) => transport.post<ControlNodeRecord>(ops.controlNodes.register, payload),
    heartbeatControlNode: (nodeId: string) =>
      transport.post<ControlNodeRecord>(ops.controlNodes.heartbeat(nodeId)),
    deleteControlNode: (nodeId: string) =>
      transport.delete<void>(ops.controlNodes.delete(nodeId)),
    runtimeDefaults: () =>
      transport.get<RuntimeDefaultsSnapshot>(ops.runtimeDefaults.snapshot),
    optionalFeatures: () =>
      transport.get<OptionalFeaturesSnapshot>(ops.optionalFeatures.snapshot),
    listSecurityEvents: (limit = 50) =>
      transport.get<SecurityEventRecord[]>(
        `${ops.securityEvents.list}${query({ limit: String(limit) })}`,
      ),
    listAuditEvents: (limit = 50) =>
      transport.get<AuditEventRecord[]>(
        `${ops.auditEvents.list}${query({ limit: String(limit) })}`,
      ),
  };
}

export function createWebFrameworkAdminBackendSdkFromEnv(
  baseUrl = import.meta.env.VITE_SDKWORK_WEB_FRAMEWORK_BACKEND_API_BASE_URL ?? "",
) {
  return createWebFrameworkAdminBackendSdk(baseUrl);
}
