import { StrictMode, useEffect, useRef, useState } from "react";

import { createRoot } from "react-dom/client";

import {
  useWebFrameworkAdmin,
  type WebFrameworkAdminTab,
} from "./hooks/useWebFrameworkAdmin";
import { messages } from "./i18n/messages";
import "./styles.css";

const EDITABLE_TABS: ReadonlySet<WebFrameworkAdminTab> = new Set([
  "cors",
  "rateLimit",
  "tenant",
  "nodes",
]);

function App() {
  const { loadTab, savePayload, heartbeatNode, deleteNode, visibleTabs, tabLabels } =
    useWebFrameworkAdmin();

  const [tab, setTab] = useState<WebFrameworkAdminTab>("defaults");
  const [environment, setEnvironment] = useState("prod");
  const [json, setJson] = useState("{}");
  const [output, setOutput] = useState<string>(messages.loading);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  // Epoch guard: only the most recent refresh may commit its result, preventing
  // stale responses (e.g. a slow previous tab) from overwriting the current view
  // when the user switches tabs/environments quickly (FRONTEND_CODE_SPEC §4).
  const refreshEpoch = useRef(0);

  async function refresh() {
    const epoch = ++refreshEpoch.current;
    setError(null);
    setIsLoading(true);
    try {
      const data = await loadTab(tab, environment);
      if (epoch !== refreshEpoch.current) {
        return;
      }
      setOutput(JSON.stringify(data, null, 2));
    } catch (err) {
      if (epoch !== refreshEpoch.current) {
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setOutput(messages.empty);
    } finally {
      if (epoch === refreshEpoch.current) {
        setIsLoading(false);
      }
    }
  }

  useEffect(() => {
    if (!visibleTabs.includes(tab)) {
      setTab(visibleTabs[0] ?? "defaults");
    }
  }, [visibleTabs, tab]);

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, environment]);

  async function save() {
    setError(null);
    try {
      const payload = JSON.parse(json);
      await savePayload(tab, payload);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function nodeAction(action: "heartbeat" | "delete") {
    setError(null);
    try {
      const payload = JSON.parse(json) as { node_id?: string };
      if (!payload.node_id) {
        throw new Error(messages.nodeIdRequired);
      }
      if (action === "heartbeat") {
        await heartbeatNode(payload.node_id);
      } else {
        await deleteNode(payload.node_id);
      }
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="layout" data-testid="web-framework-console">
      <header>
        <h1>{messages.appTitle}</h1>
        <p>{messages.appSubtitle}</p>
      </header>
      <nav>
        {visibleTabs.map((id) => (
          <button
            key={id}
            className={tab === id ? "active" : ""}
            onClick={() => setTab(id)}
            type="button"
          >
            {tabLabels[id]}
          </button>
        ))}
      </nav>
      <section className="toolbar">
        <label>
          {messages.environment}
          <select value={environment} onChange={(e) => setEnvironment(e.target.value)}>
            <option value="dev">dev</option>
            <option value="test">test</option>
            <option value="prod">prod</option>
          </select>
        </label>
        <button type="button" onClick={() => void refresh()}>
          {messages.refresh}
        </button>
        {EDITABLE_TABS.has(tab) && (
          <>
            <button type="button" onClick={() => void save()}>
              {messages.saveJson}
            </button>
            {tab === "nodes" && (
              <>
                <button type="button" onClick={() => void nodeAction("heartbeat")}>
                  {messages.heartbeatNode}
                </button>
                <button type="button" onClick={() => void nodeAction("delete")}>
                  {messages.deleteNode}
                </button>
              </>
            )}
          </>
        )}
      </section>
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      <div className="panels">
        {EDITABLE_TABS.has(tab) && (
          <textarea
            value={json}
            onChange={(e) => setJson(e.target.value)}
            placeholder={messages.jsonPlaceholder}
            rows={12}
          />
        )}
        <pre aria-busy={isLoading || undefined}>{output}</pre>
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
