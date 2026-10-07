import { useCallback, useEffect, useRef, useState } from "react";
import { api, type KairoError } from "../api/client";
import { pickSchemaFile, pickSchemaSavePath } from "../api/dialogs";
import type { ApplyPlan, ApplyReport, ConnectionInfo, SchemaCheck } from "../api/types";
import { CodeEditor, type CodeEditorHandle } from "../components/CodeEditor";
import { CliHint, CodeBlock, CopyButton, Tabs, Window, tabPanelProps } from "../components/Controls";
import { Icon } from "../components/Icon";
import { Modal } from "../components/Modal";
import { ErrorNotice, Notice } from "../components/Notice";
import { toKairoError } from "../lib/errors";
import { formatDuration, plural } from "../lib/format";
import { useApp } from "../state/app";

const STARTER = `// Define tables here, then apply them to the open database.
// Types: string, int, float, bool, blob, timestamp
// Modifiers: [primary], [required], [unique]

table users {
  id: int [primary]
  name: string [required]
  email: string [unique]
  active: bool = true
}
`;

const VALIDATE_DELAY_MS = 200;

type PreviewTab = "tables" | "sql";

interface OpenFile {
  path: string | null;
  name: string;
}

export function SchemaView() {
  const app = useApp();
  const { active, log, logError, catalogChanged } = app;
  const editor = useRef<CodeEditorHandle>(null);
  const [text, setText] = useState(STARTER);
  const [saved, setSaved] = useState(STARTER);
  const [file, setFile] = useState<OpenFile>({ path: null, name: "untitled.kairo" });
  const [check, setCheck] = useState<SchemaCheck | null>(null);
  const [tab, setTab] = useState<PreviewTab>("tables");
  const [plan, setPlan] = useState<ApplyPlan | null>(null);
  const [planning, setPlanning] = useState(false);
  const [applying, setApplying] = useState(false);
  const [report, setReport] = useState<ApplyReport | null>(null);
  const [error, setError] = useState<KairoError | null>(null);
  // An action waiting on "discard unsaved changes?".
  const [discard, setDiscard] = useState<(() => void) | null>(null);

  const dirty = text !== saved;
  const connectionId = active?.id ?? null;

  // Validate with the real parser a moment after typing stops. The dialect
  // of the preview follows the open database.
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      api
        .validateSchema(text, connectionId)
        .then((result) => !cancelled && setCheck(result))
        .catch((raw) => !cancelled && logError("Validation failed", raw));
    }, VALIDATE_DELAY_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [text, connectionId, logError]);

  const load = useCallback((content: string, next: OpenFile, isSaved: boolean) => {
    setText(content);
    setSaved(isSaved ? content : "");
    setFile(next);
    setReport(null);
    setError(null);
  }, []);

  const openPath = useCallback(
    async (path: string) => {
      try {
        const opened = await api.readSchemaFile(path);
        load(opened.content, { path: opened.path, name: opened.name }, true);
        log("info", `Opened ${opened.name}`, opened.path);
      } catch (raw) {
        setError(toKairoError(raw));
        logError("Could not open the schema", raw);
      }
    },
    [load, log, logError],
  );

  // Another view asked for a file or some text to be loaded here.
  const requested = app.schemaRequest;
  useEffect(() => {
    if (!requested) return;
    if (requested.path) {
      void openPath(requested.path);
    } else if (requested.text !== undefined) {
      load(requested.text, { path: null, name: requested.name ?? "untitled.kairo" }, false);
    }
  }, [requested, openPath, load]);

  /** Runs `action` now, or after the user agrees to lose unsaved changes. */
  const guarded = (action: () => void) => {
    if (dirty) setDiscard(() => action);
    else action();
  };

  const openFile = () =>
    guarded(async () => {
      const path = await pickSchemaFile();
      if (path) await openPath(path);
    });

  const newFile = () =>
    guarded(() => {
      load(STARTER, { path: null, name: "untitled.kairo" }, true);
      editor.current?.focus();
    });

  const saveTo = async (path: string) => {
    try {
      const written = await api.writeSchemaFile(path, text);
      setSaved(text);
      setFile({ path: written.path, name: written.name });
      log("success", `Saved ${written.name}`, written.path);
    } catch (raw) {
      setError(toKairoError(raw));
      logError("Could not save the schema", raw);
    }
  };

  const saveAs = async () => {
    const path = await pickSchemaSavePath(file.name);
    if (path) await saveTo(path);
  };

  const save = () => (file.path ? saveTo(file.path) : saveAs());

  const startApply = async () => {
    if (!active) return;
    setPlanning(true);
    setError(null);
    setReport(null);
    try {
      // Nothing changes yet: this only compares the schema with the database.
      setPlan(await api.planSchema(active.id, text));
    } catch (raw) {
      setError(toKairoError(raw));
    } finally {
      setPlanning(false);
    }
  };

  const confirmApply = async () => {
    if (!active || !plan) return;
    setApplying(true);
    try {
      const result = await api.applySchema(active.id, text, true);
      setReport(result);
      setPlan(null);
      log(
        "success",
        `Applied ${file.name} to ${result.targetName}`,
        `${plural(result.created, "table")} created, ${result.unchanged} already existed · ${formatDuration(result.elapsedMs)}`,
      );
      catalogChanged();
    } catch (raw) {
      const failure = toKairoError(raw);
      setPlan(null);
      setError(failure);
      log("error", `Applying ${file.name} failed: ${failure.message}`, failure.detail);
    } finally {
      setApplying(false);
    }
  };

  const diagnostics = check?.report.diagnostics ?? [];
  const errors = diagnostics.filter((d) => d.severity === "error").length;
  const warnings = diagnostics.length - errors;
  const valid = check?.report.valid ?? false;
  const tableCount = check?.preview?.tables.length ?? 0;

  let applyBlocked: string | null = null;
  if (!active) applyBlocked = "Open a database first";
  else if (!valid) applyBlocked = "Fix the errors first";
  else if (tableCount === 0) applyBlocked = "Define a table first";

  return (
    <div className="fill">
      <div className="toolbar">
        <button type="button" className="btn btn-outline btn-sm" onClick={newFile}>
          <Icon name="plus" size={13} /> New
        </button>
        <button type="button" className="btn btn-outline btn-sm" onClick={openFile}>
          <Icon name="folder" size={13} /> Open…
        </button>
        <button type="button" className="btn btn-outline btn-sm" onClick={save} disabled={!dirty && file.path !== null}>
          <Icon name="save" size={13} /> Save
        </button>
        <button type="button" className="btn btn-ghost btn-sm" onClick={saveAs}>
          Save as…
        </button>

        <span className="spacer" />

        <span role="status" aria-live="polite">
          {!check ? (
            <span className="muted">Checking…</span>
          ) : valid ? (
            <span className="badge badge-green">
              valid · {plural(tableCount, "table")}
              {warnings > 0 ? ` · ${plural(warnings, "warning")}` : ""}
            </span>
          ) : (
            <span className="badge badge-red">{plural(errors, "error")}</span>
          )}
        </span>

        <button
          type="button"
          className="btn btn-primary"
          onClick={startApply}
          disabled={applyBlocked !== null || planning || applying}
          title={applyBlocked ?? undefined}
        >
          {planning ? "Comparing…" : applyBlocked ?? "Apply schema…"}
        </button>
      </div>

      <div className="split grow" style={{ gridTemplateColumns: "minmax(0, 1fr) minmax(320px, 40%)" }}>
        <section className="pane" aria-label="Schema editor">
          <div className="fill pane-pad" style={{ gap: "var(--space-3)" }}>
            {error && (
              <ErrorNotice
                error={error}
                action={
                  <button type="button" className="icon-btn" aria-label="Dismiss" onClick={() => setError(null)}>
                    <Icon name="close" size={13} />
                  </button>
                }
              />
            )}
            {report && <ApplyResult report={report} onDismiss={() => setReport(null)} />}

            <Window
              className="fill"
              title={
                <>
                  {file.name}
                  {dirty && (
                    <span title="Unsaved changes" aria-label="unsaved changes">
                      {" "}
                      ●
                    </span>
                  )}
                </>
              }
              actions={
                file.path && (
                  <span className="window-title truncate" title={file.path} style={{ maxWidth: 320 }}>
                    {file.path}
                  </span>
                )
              }
            >
              <CodeEditor
                ref={editor}
                label="Kairo schema"
                language="kairo"
                value={text}
                onChange={setText}
                diagnostics={diagnostics}
                onSave={save}
              />
            </Window>

            {diagnostics.length > 0 && (
              <ul className="hairline-grid" aria-label="Problems" style={{ maxHeight: 132, overflow: "auto" }}>
                {diagnostics.map((diagnostic, index) => (
                  <li key={index}>
                    <button
                      type="button"
                      className="diag-row"
                      onClick={() => editor.current?.goTo(diagnostic.line, diagnostic.column)}
                    >
                      <span className={diagnostic.severity === "error" ? "badge badge-red" : "badge badge-amber"}>
                        {diagnostic.severity}
                      </span>
                      <span className="diag-pos">
                        {diagnostic.line}:{diagnostic.column}
                      </span>
                      <span>{diagnostic.message}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>

        <aside className="pane" aria-label="Schema preview">
          <Tabs
            label="Preview"
            idPrefix="schema"
            value={tab}
            onChange={setTab}
            items={[
              { id: "tables", label: "Preview", count: tableCount },
              { id: "sql", label: check?.preview?.dialect === "postgres" ? "PostgreSQL" : "SQLite SQL" },
            ]}
          />
          <div className="pane-scroll" {...tabPanelProps("schema", tab)}>
            <Preview check={check} tab={tab} connection={active} />
          </div>
          <div className="pane-pad" style={{ borderTop: "1px solid var(--border)" }}>
            <CliHint command={`kairo create ${file.name.replace(/\.kairo$/i, "")}`} />
          </div>
        </aside>
      </div>

      {plan && active && (
        <ApplyDialog
          plan={plan}
          fileName={file.name}
          applying={applying}
          onCancel={() => setPlan(null)}
          onConfirm={confirmApply}
        />
      )}

      {discard && (
        <Modal
          title="Discard unsaved changes?"
          onClose={() => setDiscard(null)}
          footer={
            <>
              <button type="button" className="btn btn-outline" onClick={() => setDiscard(null)} data-autofocus>
                Keep editing
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => {
                  const action = discard;
                  setDiscard(null);
                  action();
                }}
              >
                Discard changes
              </button>
            </>
          }
        >
          <p>
            <strong>{file.name}</strong> has changes that have not been saved. Opening something else
            will lose them.
          </p>
        </Modal>
      )}
    </div>
  );
}

interface PreviewProps {
  check: SchemaCheck | null;
  tab: PreviewTab;
  connection: ConnectionInfo | null;
}

/** What the parser understood, before anything touches a database. */
function Preview({ check, tab, connection }: PreviewProps) {
  if (!check) return <p className="pane-pad muted">Checking…</p>;

  const preview = check.preview;
  if (!preview) {
    return (
      <p className="pane-pad muted">
        The preview appears once the schema parses. The problems are listed under the editor.
      </p>
    );
  }
  if (preview.tables.length === 0) {
    return (
      <p className="pane-pad muted">
        No tables yet. Start with <code>table name {"{ field: type }"}</code>.
      </p>
    );
  }

  if (tab === "sql") {
    return (
      <div className="stack pane-pad">
        <p className="hint">
          {connection
            ? `What applying will run on ${connection.name}. Existing tables are left as they are.`
            : "Generated for SQLite. Open a PostgreSQL database to see its dialect."}
        </p>
        <Window title="generated.sql" actions={<CopyButton text={preview.sql} label="Copy SQL" />}>
          <div className="window-body">
            <CodeBlock code={preview.sql} language="sql" />
          </div>
        </Window>
      </div>
    );
  }

  return (
    <div className="stack pane-pad">
      {preview.tables.map((table) => (
        <section key={`${table.name}-${table.line}`} className="hairline-grid" aria-label={`Table ${table.name}`}>
          <div className="list-row">
            <Icon name="table" size={14} />
            <strong className="truncate">{table.name}</strong>
            <span className="spacer" />
            <span className="muted mono">line {table.line}</span>
          </div>
          {table.fields.map((field) => (
            <div key={`${field.name}-${field.line}`} className="list-row" style={{ flexWrap: "wrap" }}>
              <span className="mono">{field.name}</span>
              <span className={field.knownType ? "badge badge-green" : "badge badge-amber"}>
                {field.typeName}
              </span>
              <span className="muted mono" title="Column type in the database">
                → {field.sqlType}
              </span>
              <span className="spacer" />
              {field.primary && <span className="badge badge-accent">primary</span>}
              {field.required && <span className="badge badge-amber">required</span>}
              {field.unique && <span className="badge">unique</span>}
              {field.defaultValue !== null && (
                <span className="badge" title="Default value">
                  = {field.defaultValue}
                </span>
              )}
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

interface ApplyDialogProps {
  plan: ApplyPlan;
  fileName: string;
  applying: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

/** Shows exactly what applying will do, and to which database, before it does it. */
function ApplyDialog({ plan, fileName, applying, onCancel, onConfirm }: ApplyDialogProps) {
  const nothingToDo = plan.creates === 0;

  return (
    <Modal
      title={`Apply ${fileName}?`}
      onClose={onCancel}
      wide
      footer={
        <>
          {applying && <span className="spinner" aria-hidden="true" />}
          <span className="spacer" />
          <button type="button" className="btn btn-outline" onClick={onCancel} data-autofocus>
            Cancel
          </button>
          <button type="button" className="btn btn-primary" onClick={onConfirm} disabled={applying || nothingToDo}>
            {applying
              ? "Applying…"
              : nothingToDo
                ? "Nothing to create"
                : `Create ${plural(plan.creates, "table")} in ${plan.targetName}`}
          </button>
        </>
      }
    >
      <div className="stack">
        <dl className="kv">
          <dt>Target database</dt>
          <dd>
            <strong>{plan.targetName}</strong>{" "}
            <span className="badge badge-accent">{plan.engine === "sqlite" ? "SQLite" : "PostgreSQL"}</span>
          </dd>
          <dt>Location</dt>
          <dd className="mono">{plan.targetLocation}</dd>
        </dl>

        <Notice
          tone={nothingToDo ? "info" : "warning"}
          title={
            nothingToDo
              ? "Every table in this schema already exists"
              : `${plural(plan.creates, "table")} will be created`
          }
        >
          {plan.existing > 0 && (
            <p className="notice-hint">
              {plural(plan.existing, "table")} already exist{plan.existing === 1 ? "s" : ""} and will
              be left exactly as {plan.existing === 1 ? "it is" : "they are"}. Kairo creates tables;
              it does not alter existing ones.
            </p>
          )}
          {!nothingToDo && (
            <p className="notice-hint">All of it runs in one transaction: if one table fails, none are created.</p>
          )}
        </Notice>

        <div className="hairline-grid">
          {plan.items.map((item) => (
            <div key={item.table} className="plan-item">
              <div className="row">
                <strong className="mono">{item.table}</strong>
                <span className="spacer" />
                {item.action === "create" ? (
                  <span className="badge badge-green">will be created</span>
                ) : (
                  <span className="badge badge-amber">
                    exists, unchanged{item.differences.length === 0 ? " · matches" : ""}
                  </span>
                )}
              </div>
              {item.differences.length > 0 && (
                <ul>
                  {item.differences.map((difference) => (
                    <li key={difference}>{difference}</li>
                  ))}
                </ul>
              )}
            </div>
          ))}
        </div>

        <Window title="what will run" actions={<CopyButton text={plan.sql} label="Copy SQL" />}>
          <div className="window-body" style={{ maxHeight: 220 }}>
            <CodeBlock code={plan.sql} language="sql" />
          </div>
        </Window>
      </div>
    </Modal>
  );
}

function ApplyResult({ report, onDismiss }: { report: ApplyReport; onDismiss: () => void }) {
  const created = report.items.filter((item) => item.action === "create").map((item) => item.table);
  return (
    <Notice
      tone="success"
      title={`Applied to ${report.targetName}`}
      action={
        <button type="button" className="icon-btn" aria-label="Dismiss" onClick={onDismiss}>
          <Icon name="close" size={13} />
        </button>
      }
    >
      {report.created > 0
        ? `Created ${created.join(", ")}.`
        : "No tables needed creating."}
      {report.unchanged > 0 ? ` ${plural(report.unchanged, "table")} already existed and were left unchanged.` : ""}
      <span className="muted"> {formatDuration(report.elapsedMs)}</span>
    </Notice>
  );
}
