import type { SdkWorkApiResponse, SdkWorkProblemDetail } from "../../api/types";
import {
  type BackendTokenProvider,
  resolveBackendTokenProvider,
} from "../auth/token-provider";
import { SDKWORK_SUCCESS_CODE } from "../../api/httpApiConstants";

export type BackendSdkTransport = {
  get<T>(path: string): Promise<T>;
  put<T>(path: string, payload: unknown): Promise<T>;
  post<T>(path: string, payload?: unknown): Promise<T>;
  delete<T>(path: string): Promise<T>;
};

export class BackendSdkError extends Error {
  readonly status: number;
  readonly code?: number;
  readonly problemType?: string;
  readonly traceId?: string;

  constructor(
    message: string,
    status: number,
    options?: { code?: number; problemType?: string; traceId?: string },
  ) {
    super(message);
    this.name = "BackendSdkError";
    this.status = status;
    this.code = options?.code;
    this.problemType = options?.problemType;
    this.traceId = options?.traceId;
  }
}

/** Resolve per-request auth headers from the active token provider (no credential caching). */
function dualTokenHeaders(provider: BackendTokenProvider): Record<string, string> {
  const headers: Record<string, string> = { "content-type": "application/json" };
  const credentials = provider.getCredentials();
  if (credentials?.authToken) {
    headers["Authorization"] = `Bearer ${credentials.authToken}`;
  }
  if (credentials?.accessToken) {
    headers["Access-Token"] = credentials.accessToken;
  }
  return headers;
}

/** Internal transport for backend SDK facades. UI code must consume SDK methods, not this layer. */
export function createBackendSdkTransport(
  baseUrl: string,
  provider: BackendTokenProvider = resolveBackendTokenProvider(),
): BackendSdkTransport {
  const base = baseUrl.replace(/\/$/, "");

  async function request<T>(path: string, init?: RequestInit): Promise<T> {
    let response: Response;
    try {
      response = await fetch(`${base}${path}`, {
        ...init,
        method: init?.method,
        body: init?.body,
        headers: {
          ...dualTokenHeaders(provider),
          ...(init?.headers ?? {}),
        },
      });
    } catch (cause) {
      throw new BackendSdkError(
        cause instanceof Error ? cause.message : "backend SDK request failed: network error",
        0,
      );
    }

    if (response.status === 401) {
      provider.onUnauthorized();
    }

    const contentType = response.headers.get("content-type") ?? "";
    if (!response.ok) {
      if (contentType.includes("application/problem+json")) {
        const problem = (await response.json()) as SdkWorkProblemDetail;
        throw new BackendSdkError(
          problem.detail ?? problem.title ?? `backend SDK request failed: ${response.status}`,
          response.status,
          {
            code: problem.code,
            problemType: problem.type,
            traceId: problem.traceId,
          },
        );
      }
      throw new BackendSdkError(
        `backend SDK request failed: ${response.status}`,
        response.status,
      );
    }
    if (response.status === 204) {
      return undefined as T;
    }
    const body = (await response.json()) as SdkWorkApiResponse<T>;
    if (body.code !== SDKWORK_SUCCESS_CODE) {
      throw new BackendSdkError(
        `backend SDK request returned non-success code: ${body.code}`,
        response.status,
        { traceId: body.traceId },
      );
    }
    return body.data;
  }

  return {
    get: <T>(path: string) => request<T>(path),
    put: <T>(path: string, payload: unknown) =>
      request<T>(path, { method: "PUT", body: JSON.stringify(payload) }),
    post: <T>(path: string, payload?: unknown) =>
      request<T>(path, {
        method: "POST",
        body: payload === undefined ? undefined : JSON.stringify(payload),
      }),
    delete: <T>(path: string) => request<T>(path, { method: "DELETE" }),
  };
}

function query(params: Record<string, string | undefined>) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value) {
      search.set(key, value);
    }
  }
  const rendered = search.toString();
  return rendered ? `?${rendered}` : "";
}

export { query };
