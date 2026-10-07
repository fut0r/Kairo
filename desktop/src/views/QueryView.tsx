import { useCallback, useEffect, useRef, useState } from "react";
import { api, type KairoError } from "../api/client";
import type { ConnectionInfo, HistoryEntry, PreparedQuery, QueryOutcome, Risk } from "../api/types";
import { CodeEditor, type CodeEditorHandle } from "../components/CodeEditor";
import { Window } from "../components/Controls";
import { DataGrid } from "../components/DataGrid";
import { Icon } from "../components/Icon";
import { Modal } from "../components/Modal";
import { ErrorNotice, Loading, Notice } from "../components/Notice";
import { toKairoError } from "../lib/errors";
import { formatAgo, formatDuration, plural } from "../lib/format";
import { useApp } from "../state/app";
import { NoDatabase } from "./NoDatabase";

export function QueryView() {
  const { active } = useApp();
  if (!active) return <NoDatabase action="run queries" />;
  // A separate editor and history for each database.
  return <QueryWorkspace key={active.workspaceKey} connection={active} />;
}

function QueryWorkspace({ connection }: { connection: ConnectionInfo }) {
  const app = useApp();
  const { settings, logError, log, catalogChanged } = app;
  const editor = useRef<CodeEditorHandle>(null);
  const [sql, setSql] = useState("");
  const [running, setRunning] = useState(false);
  const [outcome, setOutcome] = useState<QueryOutcome | null>(null);
  const [error, setError] = useState<KairoError | null>(null);
  const [pending, setPending] = useState<PreparedQuery | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  const modifier = app.info?.platform === "macos" ? "⌘" : "Ctrl";

  const loadHistory = useCallback(async () => {
    try {
      setHistory(await api.listHistory(connection.workspaceKey));
    } catch (raw) {
      logError("Could not read query history", raw);
    }
  }, [connection.workspaceKey, logError]);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  // Another view sent SQL here.
  const requested = app.queryRequest;
  useEffect(() => {
    if (!requested) return;
    setSql(requested.sql);
    editor.current?.focus();
  }, [requested]);

  const execute = async (text: string, acknowledge: Risk | null) => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.runQuery(connection.id, text, acknowledge);
      setOutcome(result);
      const summary =
        result.rowsAffected !== null && result.columns.length === 0
          ? `${plural(result.rowsAffected, "row")} affected`
          : plural(result.rowCount, "row");
      log("success", `Query finished: ${summary} in ${formatDuration(result.elapsedMs)}`);
      // Anything but a read may have changed tables or row counts.
      if (result.risk !== "read") catalogChanged();
    } catch (raw) {
      const failure = toKairoError(raw);
      setOutcome(null);
      setError(failure);
      log("error", `Query failed: ${failure.message}`, failure.detail);
    } finally {
      setRunning(false);
      void loadHistory();
    }
  };

  const run = async () => {
    const text = sql.trim();
    if (!text || running) return;

    let prepared: PreparedQuery;
    try {
      prepared = await api.analyzeQuery(text);
    } catch (raw) {
      setOutcome(null);
      setError(toKairoError(raw));
      return;
    }

    const risk = prepared.analysis.risk;
    const mustAsk = risk === "destructive" || (risk === "write" && settings.confirmWrites);
    if (mustAsk) {
      setPending(prepared);
    } else {
      // A write that needs no confirmation still states what it is. The
      // Rust side checks this against its own analysis.
      await execute(text, risk === "read" ? null : risk);
    }
  };

  const confirm = async () => {
    if (!pending) return;
    const risk = pending.analysis.risk;
    setPending(null);
    await execute(sql.trim(), risk);
  };

  return (
    <div className="split" style={{ gridTemplateColumns: "minmax(0, 1fr) 300px" }}>
      <section className="pane" aria-label="Query">
        <div className="toolbar">
          <button
            type="button"
            className="btn btn-primary"
            onClick={run}
            disabled={running || sql.trim() === ""}
            aria-keyshortcuts="Control+Enter Meta+Enter"
          >
            <Icon name="play" size={13} />
            {running ? "Running…" : "Run"}
          </button>
          <span className="hint">
            <kbd>{modifier}</kbd> <kbd>Enter</kbd>
          </span>
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            onClick={() => {
              setSql("");
              editor.current?.focus();
            }}
            disabled={sql === ""}
          >
            Clear
          </button>
          <span className="spacer" />
          <span className="hint">
            Keeps up to {settings.maxRows.toLocaleString("en-US")} rows · stops after{" "}
            {settings.queryTimeoutSecs} s
          </span>
        </div>

        <div className="fill pane-pad" style={{ gap: "var(--space-3)" }}>
          <Window title="query.sql" className="query-editor">
            <CodeEditor
              ref={editor}
              label="SQL query"
              language="sql"
              value={sql}
              onChange={setSql}
              onRun={run}
              placeholder={"SELECT * FROM users LIMIT 50\n\n-- or the short form:  from users where age > 18"}
            />
          </Window>

          <div className="fill" aria-live="polite" aria-busy={running}>
            <Results running={running} outcome={outcome} error={error} modifier={modifier} />
          </div>
        </div>
      </section>

      <aside className="pane" aria-label="Query history">
        <div className="toolbar">
          <h2 style={{ fontSize: "var(--text-md)" }}>History</h2>
          <span className="spacer" />
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            disabled={history.length === 0}
            onClick={async () => {
              try {
                await api.clearHistory(connection.workspaceKey);
                setHistory([]);
              } catch (raw) {
                logError("Could not clear history", raw);
              }
            }}
          >
            Clear
          </button>
        </div>
        <div className="pane-scroll">
          {history.length === 0 ? (
            <p className="pane-pad muted">
              Queries you run against {connection.name} are listed here. Click one to bring it back.
            </p>
          ) : (
            <ul>
              {history.map((entry) => (
                <li key={`${entry.at}-${entry.sql}`}>
                  <button
                    type="button"
                    className="history-item"
                    onClick={() => {
                      setSql(entry.sql);
                      editor.current?.focus();
                    }}
                    title={entry.error ?? entry.sql}
                  >
                    <span className="history-sql">{entry.sql}</span>
                    <span className="history-meta">
                      <span className={entry.ok ? undefined : "level-error"} style={entry.ok ? undefined : { color: "var(--red)" }}>
                        {entry.ok ? "ok" : "failed"}
                      </span>
                      {entry.rowsAffected !== null && entry.risk !== "read" ? (
                        <span>{plural(entry.rowsAffected, "row")} affected</span>
                      ) : (
                        entry.rowCount !== null && <span>{plural(entry.rowCount, "row")}</span>
                      )}
                      {entry.elapsedMs !== null && <span>{formatDuration(entry.elapsedMs)}</span>}
                      <span className="spacer" />
                      <span>{formatAgo(entry.at)}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>

      {pending && (
        <ConfirmRun
          prepared={pending}
          connection={connection}
          onCancel={() => setPending(null)}
          onConfirm={confirm}
        />
      )}
    </div>
  );
}

interface ResultsProps {
  running: boolean;
  outcome: QueryOutcome | null;
  error: KairoError | null;
  modifier: string;
}

function Results({ running, outcome, error, modifier }: ResultsProps) {
  if (running) return <Loading label="Running…" />;
  if (error) return <ErrorNotice error={error} />;

  if (!outcome) {
    return (
      <div className="empty" style={{ padding: "var(--space-5) 0" }}>
        <h2 style={{ fontSize: "var(--text-lg)" }}>Results appear here</h2>
        <p>
          Write SQL above and press <kbd>{modifier}</kbd> <kbd>Enter</kbd>. Reads run straight away.
          Anything that changes data asks first.
        </p>
      </div>
    );
  }

  const translated = outcome.translated && (
    <div className="row">
      <span className="hint">Ran as:</span>
      <code className="code-chip truncate">{outcome.executedSql}</code>
    </div>
  );

  // A statement with no result set: report what it did.
  if (outcome.columns.length === 0) {
    return (
      <div className="stack">
        <Notice tone="success" title="Statement finished">
          {outcome.rowsAffected !== null ? `${plural(outcome.rowsAffected, "row")} affected · ` : ""}
          {outcome.statementCount > 1 ? `${outcome.statementCount} statements · ` : ""}
          {formatDuration(outcome.elapsedMs)}
        </Notice>
        {translated}
      </div>
    );
  }

  return (
    <div className="fill" style={{ gap: "var(--space-2)" }}>
      <div className="row row-wrap">
        <strong>{plural(outcome.rowCount, "row")}</strong>
        <span className="muted">
          {plural(outcome.columns.length, "column")} · {formatDuration(outcome.elapsedMs)}
          {outcome.statementCount > 1
            ? ` · ${outcome.statementCount} statements, showing the last result`
            : ""}
        </span>
        {outcome.truncated && (
          <span className="badge badge-amber" title="Add a LIMIT, or raise the cap in Settings">
            more rows exist, showing the first {outcome.rowCount.toLocaleString("en-US")}
          </span>
        )}
      </div>
      {translated}
      {outcome.rowCount === 0 ? (
        <Notice tone="info" title="The query returned no rows">
          <span className="muted">
            Columns: {outcome.columns.map((column) => column.name).join(", ")}
          </span>
        </Notice>
      ) : (
        <DataGrid label="Query results" columns={outcome.columns} rows={outcome.rows} />
      )}
    </div>
  );
}

interface ConfirmRunProps {
  prepared: PreparedQuery;
  connection: ConnectionInfo;
  onCancel: () => void;
  onConfirm: () => void;
}

/** Asks before SQL that changes data runs, with the reasons the analysis gave. */
function ConfirmRun({ prepared, connection, onCancel, onConfirm }: ConfirmRunProps) {
  const destructive = prepared.analysis.risk === "destructive";
  const risky = prepared.analysis.statements.filter((s) => s.risk !== "read");
  // Gated only because Kairo cannot classify it. Usually a typo, so it must
  // not be described as something known to destroy data.
  const unknown =
    destructive && risky.filter((s) => s.risk === "destructive").every((s) => !s.recognized);

  const title = unknown
    ? "Run a statement Kairo does not recognise?"
    : destructive
      ? "Run a destructive statement?"
      : "Run a statement that changes data?";
  const warning = unknown
    ? "Kairo cannot tell what this does, so it asks first. If it is a typo, cancel and fix it."
    : destructive
      ? "This can remove or overwrite data, and Kairo cannot undo it."
      : "This will change the database.";

  return (
    <Modal
      title={title}
      onClose={onCancel}
      wide
      footer={
        <>
          <button type="button" className="btn btn-outline" onClick={onCancel} data-autofocus>
            Cancel
          </button>
          <button
            type="button"
            className={destructive ? "btn btn-danger" : "btn btn-primary"}
            onClick={onConfirm}
          >
            {destructive ? "Run anyway" : "Run"}
          </button>
        </>
      }
    >
      <div className="stack">
        <Notice tone={destructive && !unknown ? "error" : "warning"} title={warning}>
          <ul className="plan-item" style={{ padding: 0 }}>
            {prepared.analysis.reasons.map((reason) => (
              <li key={reason} style={{ listStyle: "none" }}>
                {reason}
              </li>
            ))}
          </ul>
        </Notice>

        <dl className="kv">
          <dt>Database</dt>
          <dd>
            <strong>{connection.name}</strong>{" "}
            <span className="badge badge-accent">
              {connection.engine === "sqlite" ? "SQLite" : "PostgreSQL"}
            </span>
          </dd>
          <dt>Location</dt>
          <dd className="mono">{connection.location}</dd>
        </dl>

        <div className="hairline-grid">
          {risky.map((statement, index) => (
            <div key={index} className="plan-item">
              <div className="row">
                <span className={statement.risk === "destructive" ? "badge badge-red" : "badge badge-amber"}>
                  {statement.risk}
                </span>
                <code className="mono truncate">{statement.preview}</code>
              </div>
            </div>
          ))}
        </div>
      </div>
    </Modal>
  );
}
