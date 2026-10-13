// The assistant: model settings, the AGENTS.md files it follows, and the
// conversation, including a live log of every tool it calls.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  agentCancel,
  agentConnect,
  agentPrompt,
  agentReset,
  apiKeyEnv,
  models as fetchModels,
  onAgent,
  providers as fetchProviders,
  startupModel,
  type ModelChoice,
  type Run,
  type View,
} from "../api";
import { Combobox } from "./Combobox";

export type ChatEntry =
  | { kind: "user" | "assistant" | "error" | "info"; text: string }
  | { kind: "tool"; name: string; arguments: string; result: string | null; error: string | null };

export interface Assistant {
  entries: ChatEntry[];
  busy: boolean;
  connected: string | null;
  send: (text: string) => void;
  connect: (model: string, apiKey: string | null) => void;
  reset: () => void;
}

/** The conversation, kept by the app so hiding the panel loses nothing. */
export function useAssistant(): Assistant {
  const [entries, setEntries] = useState<ChatEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [connected, setConnected] = useState<string | null>(null);
  const add = (entry: ChatEntry) => setEntries((entries) => [...entries, entry]);

  useEffect(() => {
    const unlisten = onAgent((event) => {
      switch (event.kind) {
        case "connected":
          setConnected(event.model);
          add({ kind: "info", text: `Connected to ${event.model}.` });
          break;
        case "tool":
          add({ ...event, kind: "tool" });
          break;
        case "reply":
          setBusy(false);
          add({ kind: "assistant", text: event.text });
          break;
        case "failed":
          setBusy(false);
          add({ kind: "error", text: event.message });
          break;
        case "cancelled":
          setBusy(false);
          add({ kind: "info", text: "Stopped." });
          break;
      }
    });
    return () => void unlisten.then((stop) => stop());
  }, []);

  return {
    entries,
    busy,
    connected,
    send: useCallback((text: string) => {
      add({ kind: "user", text });
      setBusy(true);
      void agentPrompt(text);
    }, []),
    connect: useCallback((model: string, apiKey: string | null) => void agentConnect(model, apiKey), []),
    reset: useCallback(() => {
      setEntries([]);
      void agentReset();
    }, []),
  };
}

export function AssistantPanel({ view, run, assistant }: { view: View; run: Run; assistant: Assistant }) {
  return (
    <div className="assistant">
      <header className="panel-header">
        <span className="section-title">Assistant</span>
        <span className="spacer" />
        <button className="icon" title="New conversation" onClick={assistant.reset}>
          <span className="codicon codicon-trash" />
        </button>
      </header>
      <Section title={assistant.connected ? `Model: ${assistant.connected}` : "Model: not connected"} defaultOpen>
        <ModelSettings assistant={assistant} />
      </Section>
      <Section title={instructionsTitle(view)}>
        <Instructions view={view} run={run} />
      </Section>
      <Chat assistant={assistant} />
    </div>
  );
}

function Section({ title, defaultOpen = false, children }: { title: string; defaultOpen?: boolean; children: ReactNode }) {
  return (
    <details className="section" open={defaultOpen}>
      <summary>{title}</summary>
      <div className="section-body">{children}</div>
    </details>
  );
}

function ModelSettings({ assistant }: { assistant: Assistant }) {
  const [providerList, setProviderList] = useState<string[]>([]);
  const [provider, setProvider] = useState("");
  const [modelList, setModelList] = useState<ModelChoice[]>([]);
  const [model, setModel] = useState("");
  const [custom, setCustom] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [keyEnv, setKeyEnv] = useState<string | null>(null);

  useEffect(() => {
    void fetchProviders().then(setProviderList);
    // `JSON_DIFF_MODEL=provider:model` connects at startup.
    void startupModel().then(async (reference) => {
      if (!reference) return;
      const split = reference.indexOf(":");
      const [vendor, id] = split < 0 ? [reference, ""] : [reference.slice(0, split), reference.slice(split + 1)];
      setProvider(vendor);
      setModel(id);
      const listed = await fetchModels(vendor);
      setCustom(id !== "" && !listed.some((choice) => choice.id === id));
      assistant.connect(reference, null);
    });
  }, []);

  useEffect(() => {
    setModelList([]);
    setKeyEnv(null);
    if (!provider) return;
    void fetchModels(provider).then(setModelList);
    void apiKeyEnv(provider).then(setKeyEnv);
  }, [provider]);

  const listed = modelList.length > 0;
  const reference = provider && model.trim() ? `${provider}:${model.trim()}` : null;
  const providerOptions = useMemo(() => providerList.map((id) => ({ id, label: id })), [providerList]);
  const modelOptions = useMemo(
    () => modelList.map((choice) => ({ id: choice.id, label: choice.name })),
    [modelList],
  );

  return (
    <div className="model-settings">
      <label>Provider</label>
      <Combobox
        options={providerOptions}
        value={provider}
        placeholder="Choose…"
        searchPlaceholder="Search providers"
        onPick={(id) => {
          if (id === provider) return;
          setProvider(id);
          setModel("");
          setCustom(false);
        }}
      />
      <label>Model</label>
      {!provider ? (
        <Combobox options={[]} value="" placeholder="Choose a provider first" searchPlaceholder="" disabled onPick={() => {}} />
      ) : custom || !listed ? (
        <div className="row">
          <input
            placeholder="model id, e.g. llama3.2"
            value={model}
            onChange={(event) => setModel(event.target.value)}
          />
          {listed && (
            <button
              className="small"
              title="Pick from the provider's models"
              onClick={() => {
                setCustom(false);
                setModel("");
              }}
            >
              List
            </button>
          )}
        </div>
      ) : (
        <Combobox
          options={modelOptions}
          value={model}
          placeholder="Choose…"
          searchPlaceholder="Search models, e.g. gpt"
          onPick={setModel}
          footer={{
            label: "Other (type a model id)…",
            onPick: (query) => {
              setCustom(true);
              setModel(query);
            },
          }}
        />
      )}
      <input
        className="wide"
        type="password"
        placeholder={keyEnv ? `API key (default: $${keyEnv})` : "API key (optional)"}
        value={apiKey}
        onChange={(event) => setApiKey(event.target.value)}
      />
      <button
        className="wide"
        disabled={!reference || assistant.busy}
        onClick={() => reference && assistant.connect(reference, apiKey.trim() || null)}
      >
        Connect
      </button>
    </div>
  );
}

function instructionsTitle(view: View) {
  const total = view.agentsMd.length;
  const enabled = view.agentsMd.filter((file) => file.enabled).length;
  if (total === 0) return "Instructions (AGENTS.md)";
  return enabled === total
    ? `Instructions (AGENTS.md): ${total}`
    : `Instructions (AGENTS.md): ${enabled} of ${total}`;
}

function Instructions({ view, run }: { view: View; run: Run }) {
  const add = async () => {
    const path = await open({
      title: "Add instructions for the assistant (AGENTS.md)",
      filters: [
        { name: "Markdown files", extensions: ["md", "markdown"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof path === "string") await run("add_agents_md", { path });
  };
  return (
    <div className="instructions">
      {view.agentsMd.length === 0 && (
        <p className="weak">
          An AGENTS.md next to the opened folders or files is used automatically. You can also add one yourself.
        </p>
      )}
      {view.agentsMd.map((file) => (
        <div key={file.path} className="instruction" title={file.path}>
          <label>
            <input
              type="checkbox"
              checked={file.enabled}
              onChange={(event) => run("set_agents_md_enabled", { path: file.path, enabled: event.target.checked })}
            />
            {file.label}
          </label>
          <span className="spacer" />
          <span className="tag">{file.source}</span>
          {file.source === "added" && (
            <button className="icon" title="Remove" onClick={() => run("remove_agents_md", { path: file.path })}>
              ×
            </button>
          )}
        </div>
      ))}
      <button title="Pick an instructions file for the assistant to follow" onClick={() => void add()}>
        Add AGENTS.md…
      </button>
    </div>
  );
}

function Chat({ assistant }: { assistant: Assistant }) {
  const [input, setInput] = useState("");
  const log = useRef<HTMLDivElement>(null);
  const ready = assistant.connected !== null && !assistant.busy;

  useEffect(() => {
    log.current?.scrollTo({ top: log.current.scrollHeight });
  }, [assistant.entries, assistant.busy]);

  const send = () => {
    const text = input.trim();
    if (!text || !ready) return;
    setInput("");
    assistant.send(text);
  };

  return (
    <div className="chat">
      <div className="chat-log" ref={log}>
        {assistant.entries.length === 0 && (
          <p className="weak">
            Ask about the diff, or to drive the app: "open ~/a and ~/b and summarise what changed", "explain change 3",
            "sort keys".
          </p>
        )}
        {assistant.entries.map((entry, index) =>
          entry.kind === "tool" ? (
            <details key={index} className={`chat-tool${entry.error ? " failed" : ""}`}>
              <summary>
                <span className="codicon codicon-tools" /> {entry.name}
                <span className="weak"> {entry.arguments}</span>
              </summary>
              <pre>{entry.error ?? entry.result}</pre>
            </details>
          ) : (
            <div key={index} className={`chat-entry ${entry.kind}`}>
              {entry.text}
            </div>
          ),
        )}
        {assistant.busy && (
          <div className="chat-entry info">
            <span className="codicon codicon-loading spin" /> working…
          </div>
        )}
      </div>
      <textarea
        rows={3}
        disabled={!ready}
        placeholder={assistant.connected ? "Message (Enter to send, Shift+Enter for a new line)" : "Connect a model first"}
        value={input}
        onChange={(event) => setInput(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            send();
          }
        }}
      />
      <div className="row">
        {assistant.busy ? (
          <button onClick={() => void agentCancel()}>Stop</button>
        ) : (
          <button disabled={!ready || !input.trim()} onClick={send}>
            Send
          </button>
        )}
      </div>
    </div>
  );
}
