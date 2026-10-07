import { useState } from "react";
import { api, type KairoError } from "../api/client";
import { pickDatabaseFile, pickNewDatabaseFile } from "../api/dialogs";
import type { ConnectRequest, ConnectionCheck } from "../api/types";
import { toKairoError } from "../lib/errors";
import { formatDuration, plural } from "../lib/format";
import { maskUrlPassword } from "../lib/mask";
import { useApp, type ConnectDialogState } from "../state/app";
import { Tabs, tabPanelProps } from "./Controls";
import { Modal } from "./Modal";
import { ErrorNotice, Notice } from "./Notice";

type Tab = ConnectDialogState["tab"];

export function ConnectionDialog() {
  const { connectDialog } = useApp();
  // Keyed so that reopening the dialog starts from a clean form.
  return connectDialog ? (
    <ConnectionForm key={`${connectDialog.tab}:${connectDialog.url ?? ""}`} initial={connectDialog} />
  ) : null;
}

function ConnectionForm({ initial }: { initial: ConnectDialogState }) {
  const app = useApp();
  const [tab, setTab] = useState<Tab>(initial.tab);
  const [path, setPath] = useState("");
  const [url, setUrl] = useState(initial.url ?? "");
  const [password, setPassword] = useState("");
  const [urlFocused, setUrlFocused] = useState(false);
  const [busy, setBusy] = useState<"test" | "connect" | null>(null);
  const [check, setCheck] = useState<ConnectionCheck | null>(null);
  const [error, setError] = useState<KairoError | null>(null);

  const reset = () => {
    setCheck(null);
    setError(null);
  };

  const request = (create = false, chosenPath = path): ConnectRequest =>
    tab === "sqlite"
      ? { kind: "sqlite", path: chosenPath, create }
      : { kind: "postgres", url, password: password || null };

  const ready = tab === "sqlite" ? path.trim() !== "" : url.trim() !== "";

  const test = async () => {
    reset();
    setBusy("test");
    try {
      setCheck(await api.testConnection(request()));
    } catch (raw) {
      setError(toKairoError(raw));
    } finally {
      setBusy(null);
    }
  };

  const open = async (toOpen: ConnectRequest) => {
    reset();
    setBusy("connect");
    try {
      // Opening is itself the test: nothing is remembered unless it succeeds.
      await app.connect(toOpen);
      app.closeConnectDialog();
      if (app.view === "overview") app.navigate("explorer");
    } catch (raw) {
      setError(toKairoError(raw));
      setBusy(null);
    }
  };

  const browse = async () => {
    const chosen = await pickDatabaseFile();
    if (chosen) {
      setPath(chosen);
      reset();
    }
  };

  const createNew = async () => {
    const chosen = await pickNewDatabaseFile();
    if (chosen) await open({ kind: "sqlite", path: chosen, create: true });
  };

  const recentFiles = app.recents.filter((item) => item.kind === "sqlite").slice(0, 5);
  const reconnecting = tab === "postgres" && Boolean(initial.url);

  return (
    <Modal
      title={reconnecting ? "Reconnect" : "Connect to a database"}
      onClose={app.closeConnectDialog}
      footer={
        <>
          {busy && <span className="spinner" aria-hidden="true" />}
          <span className="spacer" />
          <button type="button" className="btn btn-ghost" onClick={app.closeConnectDialog}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-outline"
            onClick={test}
            disabled={!ready || busy !== null}
          >
            {busy === "test" ? "Testing…" : "Test connection"}
          </button>
          <button
            type="submit"
            form="connection-form"
            className="btn btn-primary"
            disabled={!ready || busy !== null}
          >
            {busy === "connect" ? "Connecting…" : tab === "sqlite" ? "Open database" : "Connect"}
          </button>
        </>
      }
    >
      <form
        id="connection-form"
        className="stack"
        onSubmit={(event) => {
          event.preventDefault();
          if (ready && !busy) void open(request());
        }}
      >
        <Tabs
          label="Database type"
          idPrefix="connect"
          value={tab}
          onChange={(next) => {
            setTab(next);
            reset();
          }}
          items={[
            { id: "sqlite", label: "SQLite file" },
            { id: "postgres", label: "PostgreSQL" },
          ]}
        />

        {tab === "sqlite" ? (
          <div className="stack" {...tabPanelProps("connect", "sqlite")}>
            <div className="field">
              <label className="label" htmlFor="sqlite-path">
                Database file
              </label>
              <div className="row">
                <input
                  id="sqlite-path"
                  className="input input-mono"
                  value={path}
                  onChange={(event) => {
                    setPath(event.target.value);
                    reset();
                  }}
                  placeholder="C:\data\app.db"
                  spellCheck={false}
                  autoComplete="off"
                  data-autofocus
                />
                <button type="button" className="btn btn-outline" onClick={browse}>
                  Browse…
                </button>
              </div>
              <p className="hint">
                Choose an existing SQLite file, or{" "}
                <button type="button" className="btn-link" onClick={createNew}>
                  create a new one
                </button>
                .
              </p>
            </div>

            {recentFiles.length > 0 && (
              <div className="field">
                <span className="label" id="recent-files">
                  Recent files
                </span>
                <ul className="hairline-grid" aria-labelledby="recent-files">
                  {recentFiles.map((item) => (
                    <li key={item.key}>
                      <button
                        type="button"
                        className="list-row"
                        onClick={() => void open({ kind: "sqlite", path: item.target })}
                        disabled={busy !== null}
                      >
                        <span className="truncate">{item.name}</span>
                        <span className="mono muted truncate">{item.location}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        ) : (
          <div className="stack" {...tabPanelProps("connect", "postgres")}>
            <div className="field">
              <label className="label" htmlFor="pg-url">
                Connection URL
              </label>
              <input
                id="pg-url"
                className="input input-mono"
                // While the field is not being edited, a password in it is hidden.
                value={urlFocused ? url : maskUrlPassword(url)}
                onFocus={() => setUrlFocused(true)}
                onBlur={() => setUrlFocused(false)}
                onChange={(event) => {
                  setUrl(event.target.value);
                  reset();
                }}
                placeholder="postgres://user@localhost:5432/database"
                spellCheck={false}
                autoComplete="off"
                data-autofocus={reconnecting ? undefined : true}
              />
              <p className="hint">
                Add <code>?sslmode=require</code> for servers that need TLS.
              </p>
            </div>

            <div className="field">
              <label className="label" htmlFor="pg-password">
                Password
              </label>
              <input
                id="pg-password"
                className="input"
                type="password"
                value={password}
                onChange={(event) => {
                  setPassword(event.target.value);
                  reset();
                }}
                autoComplete="off"
                data-autofocus={reconnecting ? true : undefined}
              />
              <p className="hint">
                Leave empty if the URL already contains it. The password is kept in memory for this
                session only. It is never written to disk, so you will be asked again next time.
              </p>
            </div>
          </div>
        )}

        {check && (
          <Notice tone="success" title="Connection works">
            {check.serverVersion} · {plural(check.tableCount, "table")} ·{" "}
            {formatDuration(check.latencyMs)}
            <div className="notice-detail">{check.location}</div>
          </Notice>
        )}
        {error && <ErrorNotice error={error} />}
      </form>
    </Modal>
  );
}
