# sdkwork-web-chain

SDKWork web framework call chain: composable guard stages with layered
per-scope policy. A generic building block for open-API gateways — shipped
with concurrency control (bulkhead) and IP allow/deny list stages, and
composed through the `ChainStage` trait plus a `PolicyResolver` supplied by
the consuming application.

This is a business-domain guard chain complementary to the standard
`WebCallInterceptorChain` HTTP request chain in `sdkwork-web-core`; it never
redefines the standard 18-stage HTTP chain semantics.

## Building blocks

| Piece | Purpose |
|---|---|
| `CallChainBuilder` | Programmatic composition: `with_stage(...)` in any order; duplicate stage names rejected; `(stage_order, name)` deterministic execution |
| `ChainStage` | Free-form guard unit: `before` / `after` / `on_error` hooks with per-stage enablement from the effective policy |
| `ChainPolicy` / `PolicyResolver` | Layered config: built-in defaults → global policy → per-API-key overrides, field-level merge (most specific wins; explicit disablements win) |
| `ConcurrencyStage` | Bulkhead per scope (global / API key / tenant) over `ConcurrentAdmissionStore` (memory or Redis); 429 + `Retry-After` on rejection; streaming-safe release via `after`/`on_error` |
| `IpAccessStage` | Denylist-always-wins + allowlist gating; IPv4/IPv6 + CIDR via `ipnet`; IPv4-mapped IPv6 normalized; pluggable `IpExtractor` for spoof-proof client IP |

## Runtime semantics

- `before` runs enabled stages in order; the first rejection short-circuits
  and stages that already passed release in reverse order (no slot leaks).
- Stage failures and store outages **degrade open** with a warning instead of
  turning infrastructure blips into client rejections; genuine limit
  rejections are always enforced.
- Config changes are effective within one resolution TTL (the clawrouter
  integration caches store reads for 30s; see `CHAIN_POLICY_CACHE_TTL_SECS`).

## Operational constraints

- Distributed concurrency counters use Redis with a TTL of 2 hours per key;
  streaming invocations longer than the TTL can oversubscribe a budget.
  The clawrouter default stream timeout is 120s, well below the TTL — if you
  raise the stream timeout beyond 2h, shorten the window or refresh leases.
- The Redis store degrades to per-node memory when unreachable (logged);
  a fleet of N nodes may then each allow the full budget (same trade-off as
  the gateway's local-fallback rate limiter).

## Verification

```text
cargo test -p sdkwork-web-chain   # 30 unit tests: chain order/short-circuit,
                                 # policy merge, IP matching (incl. IPv6),
                                 # concurrency leases, fail-open behavior
```
