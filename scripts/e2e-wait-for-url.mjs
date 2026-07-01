/** Polls an HTTP URL until it returns a 2xx response or times out. */

export async function waitForUrl(url, timeoutMs = 120_000, intervalMs = 250) {
  const deadline = Date.now() + timeoutMs;
  let lastError = "timeout";

  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { redirect: "manual" });
      if (response.ok) {
        return;
      }
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }

  throw new Error(`timed out waiting for ${url}: ${lastError}`);
}
