import type {
  AuditEventRecord,
  ControlNodeRecord,
  CorsPolicyRecord,
  OptionalFeaturesSnapshot,
  RateLimitPolicyRecord,
  RuntimeDefaultsSnapshot,
  SdkWorkPageData,
  SecurityEventRecord,
  TenantRuntimeProfileRecord,
} from "../../api/types";
import { webFrameworkAdminOperations } from "./operations";
import { createBackendSdkTransport, query, type BackendSdkTransport } from "./transport";

export type WebFrameworkAdminBackendSdk = ReturnType<typeof createWebFrameworkAdminBackendSdk>;

const DEFAULT_PAGE_SIZE = 20;

/** Backend SDK facade for framework control-plane `/backend/v3/api/web-framework` operations. */
export function createWebFrameworkAdminBackendSdk(
  baseUrl: string,
  transport: BackendSdkTransport = createBackendSdkTransport(baseUrl),
) {
  const ops = webFrameworkAdminOperations;
  return {
    listCorsPolicies: (environment?: string, pageSize = DEFAULT_PAGE_SIZE, page = 1) =>
      transport.get<SdkWorkPageData<CorsPolicyRecord>>(
        `${ops.corsPolicies.list}${query({
          environment,
          page_size: String(pageSize),
          page: String(page),
        })}`,
      ),
    upsertCorsPolicy: (payload: CorsPolicyRecord) =>
      transport.put<CorsPolicyRecord>(ops.corsPolicies.upsert, payload),
    listRateLimitPolicies: (environment?: string, pageSize = DEFAULT_PAGE_SIZE, page = 1) =>
      transport.get<SdkWorkPageData<RateLimitPolicyRecord>>(
        `${ops.rateLimitPolicies.list}${query({
          environment,
          page_size: String(pageSize),
          page: String(page),
        })}`,
      ),
    upsertRateLimitPolicy: (payload: RateLimitPolicyRecord) =>
      transport.put<RateLimitPolicyRecord>(ops.rateLimitPolicies.upsert, payload),
    listTenantProfiles: (environment?: string, pageSize = DEFAULT_PAGE_SIZE, page = 1) =>
      transport.get<SdkWorkPageData<TenantRuntimeProfileRecord>>(
        `${ops.tenantRuntimeProfiles.list}${query({
          environment,
          page_size: String(pageSize),
          page: String(page),
        })}`,
      ),
    upsertTenantProfile: (payload: TenantRuntimeProfileRecord) =>
      transport.put<TenantRuntimeProfileRecord>(ops.tenantRuntimeProfiles.upsert, payload),
    listControlNodes: (environment?: string, pageSize = DEFAULT_PAGE_SIZE, page = 1) =>
      transport.get<SdkWorkPageData<ControlNodeRecord>>(
        `${ops.controlNodes.list}${query({
          environment,
          page_size: String(pageSize),
          page: String(page),
        })}`,
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
    listSecurityEvents: (pageSize = DEFAULT_PAGE_SIZE, cursor?: string) =>
      transport.get<SdkWorkPageData<SecurityEventRecord>>(
        `${ops.securityEvents.list}${query({
          page_size: String(pageSize),
          cursor,
        })}`,
      ),
    listAuditEvents: (pageSize = DEFAULT_PAGE_SIZE, cursor?: string) =>
      transport.get<SdkWorkPageData<AuditEventRecord>>(
        `${ops.auditEvents.list}${query({
          page_size: String(pageSize),
          cursor,
        })}`,
      ),
  };
}

export function createWebFrameworkAdminBackendSdkFromEnv(
  baseUrl = import.meta.env.VITE_SDKWORK_WEB_FRAMEWORK_BACKEND_API_BASE_URL ?? "",
) {
  return createWebFrameworkAdminBackendSdk(baseUrl);
}
