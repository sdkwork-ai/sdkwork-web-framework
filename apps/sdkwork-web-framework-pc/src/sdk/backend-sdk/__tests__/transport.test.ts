import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  createBackendSdkTransport,
  BackendSdkError,
  type BackendSdkTransport,
} from "../transport";
import type { BackendTokenProvider } from "../../auth/token-provider";

function makeFakeProvider(
  credentials: { authToken: string; accessToken: string } | null,
): BackendTokenProvider {
  return {
    getCredentials: () => credentials,
    onUnauthorized: vi.fn(),
  };
}

const okEnvelope = <T>(data: T) => ({ code: 0, data, traceId: "trace-ok" });

describe("BackendSdkError", () => {
  it("captures status, code, problemType, traceId", () => {
    const error = new BackendSdkError("not found", 404, {
      code: 40401,
      problemType: "https://sdkwork.dev/problems/not-found",
      traceId: "trace-1",
    });
    expect(error.message).toBe("not found");
    expect(error.status).toBe(404);
    expect(error.code).toBe(40401);
    expect(error.problemType).toBe("https://sdkwork.dev/problems/not-found");
    expect(error.traceId).toBe("trace-1");
    expect(error.name).toBe("BackendSdkError");
  });

  it("works with minimal arguments", () => {
    const error = new BackendSdkError("network error", 0);
    expect(error.message).toBe("network error");
    expect(error.status).toBe(0);
    expect(error.problemType).toBeUndefined();
  });
});

describe("createBackendSdkTransport", () => {
  const provider = makeFakeProvider({ authToken: "auth-jwt", accessToken: "access-jwt" });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("sends dual-token headers on GET request", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(okEnvelope([{ tenant_id: "100001" }])), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport: BackendSdkTransport = createBackendSdkTransport("http://localhost:3920", provider);
    const result = await transport.get("/backend/v3/api/web-framework/cors_policies");

    expect(result).toEqual([{ tenant_id: "100001" }]);
    const [, init] = fetchMock.mock.calls[0];
    expect(init?.headers).toMatchObject({
      Authorization: "Bearer auth-jwt",
      "Access-Token": "access-jwt",
      "content-type": "application/json",
    });
  });

  it("returns undefined for 204 No Content", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(null, { status: 204 }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", provider);
    const result = await transport.delete("/backend/v3/api/web-framework/control_nodes/node-1");

    expect(result).toBeUndefined();
  });

  it("parses Problem+json error response into BackendSdkError", async () => {
    const problem = {
      type: "https://sdkwork.dev/problems/forbidden",
      title: "Forbidden",
      detail: "cross-tenant upsert rejected",
      status: 403,
      code: 40301,
      traceId: "trace-xyz",
    };
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(problem), {
        status: 403,
        headers: { "content-type": "application/problem+json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", provider);
    await expect(transport.put("/cors_policies", {})).rejects.toMatchObject({
      name: "BackendSdkError",
      status: 403,
      message: "cross-tenant upsert rejected",
      problemType: "https://sdkwork.dev/problems/forbidden",
      traceId: "trace-xyz",
    });
  });

  it("calls provider.onUnauthorized on 401", async () => {
    const localProvider = makeFakeProvider({ authToken: "expired", accessToken: "expired" });
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ type: "missing-credentials", title: "Unauthorized", status: 401 }), {
        status: 401,
        headers: { "content-type": "application/problem+json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", localProvider);
    await expect(transport.get("/cors_policies")).rejects.toMatchObject({ status: 401 });
    expect(localProvider.onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it("wraps network errors into BackendSdkError with status 0", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", provider);
    await expect(transport.get("/cors_policies")).rejects.toMatchObject({
      name: "BackendSdkError",
      status: 0,
    });
  });

  it("throws BackendSdkError for non-problem+json HTTP errors", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ code: 42901, data: null, traceId: "trace-429" }), {
        status: 429,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", provider);
    await expect(transport.get("/cors_policies")).rejects.toMatchObject({
      name: "BackendSdkError",
      status: 429,
    });
  });

  it("sends PUT with JSON body", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(okEnvelope({ tenant_id: "100001" })), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", provider);
    await transport.put("/cors_policies", { tenant_id: "100001", environment: "prod" });

    const [, init] = fetchMock.mock.calls[0];
    expect(init?.method).toBe("PUT");
    expect(init?.body).toBe(JSON.stringify({ tenant_id: "100001", environment: "prod" }));
  });

  it("works with provider returning null credentials (anonymous)", async () => {
    const nullProvider = makeFakeProvider(null);
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(okEnvelope(okDefaults())), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const transport = createBackendSdkTransport("http://localhost:3920", nullProvider);
    const result = await transport.get("/runtime_defaults");
    expect(result).toEqual(okDefaults());

    const [, init] = fetchMock.mock.calls[0];
    expect(init?.headers).not.toHaveProperty("Authorization");
    expect(init?.headers).not.toHaveProperty("Access-Token");
  });
});

function okDefaults() {
  return {
    production_security_policy: {},
    default_security_policy: {},
    optional_features_production_sqlx: {},
  };
}
