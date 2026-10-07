import { useCallback, useEffect, useState } from "react";
import { api, type KairoError } from "../api/client";
import { pickSchemaSavePath } from "../api/dialogs";
import type { ConnectionInfo, ExportedSchema } from "../api/types";
import { CliHint, CodeBlock, CopyButton, Window } from "../components/Controls";
import { Icon } from "../components/Icon";
import { ErrorNotice, Loading, Notice } from "../components/Notice";
import { toKairoError } from "../lib/errors";
import { plural } from "../lib/format";
import { useApp } from "../state/app";
import { NoDatabase } from "./NoDatabase";

export function ExportView() {
  const { active } = useApp();
  if (!active) return <NoDatabase action="export its structure" />;
  return <ExportPanel key={active.id} connection={active} />;
}

function ExportPanel({ connection }: { connection: ConnectionInfo }) {
  const app = useApp();
  const { log, logError } = app;
  const [exported, setExported] = useState<ExportedSchema | null>(null);
  const [error, setError] = useState<KairoError | null>(null);
  const [loading, setLoading] = useState(true);
  const [savedTo, setSavedTo] = useState<string | null>(null);

  const generate = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSavedTo(null);
    try {
      // Reads the catalog only; the database is not changed.
      setExported(await api.exportSchema(connection.id));
    } catch (raw) {
      setExported(null);
      setError(toKairoError(raw));
    } finally {
      setLoading(false);
    }
  }, [connection.id]);

  useEffect(() => {
    void generate();
  }, [generate, app.catalogVersion]);

  const suggested = `${connection.name.replace(/\.[^.]+$/, "") || "schema"}.kairo`;

  const save = async () => {
    if (!exported) return;
    try {
      const path = await pickSchemaSavePath(suggested);
      if (!path) return;
      const written = await api.writeSchemaFile(path, exported.text);
      setSavedTo(written.path);
      log("success", `Exported ${connection.name} to ${written.name}`, written.path);
    } catch (raw) {
      setError(toKairoError(raw));
      logError("Could not save the export", raw);
    }
  };

  // The URL form of the command never includes a password.
  const cliTarget =
    connection.engine === "sqlite"
      ? `"${connection.location}"`
      : `"postgres://${connection.username ?? "user"}@${connection.host ?? "host"}/${connection.database ?? ""}"`;

  return (
    <div className="page-pad fill" style={{ gap: "var(--space-4)" }}>
      <div className="row row-wrap">
        <div style={{ minWidth: 0, flex: 1 }}>
          <p className="eyebrow">Structure only</p>
          <h2>
            {connection.name} as a <code style={{ fontSize: "inherit" }}>.kairo</code> schema
          </h2>
          <p className="muted">
            Tables, columns, defaults and keys. No rows are read. The result can be applied to
            another database from the Schema workspace.
          </p>
        </div>
        <button type="button" className="btn btn-outline" onClick={generate} disabled={loading}>
          <Icon name="refresh" size={14} /> Regenerate
        </button>
        <button type="button" className="btn btn-primary" onClick={save} disabled={!exported}>
          <Icon name="save" size={14} /> Save as…
        </button>
      </div>

      {error && <ErrorNotice error={error} />}
      {savedTo && (
        <Notice tone="success" title="Schema saved">
          <div className="notice-detail">{savedTo}</div>
        </Notice>
      )}
      {exported && exported.notes.length > 0 && (
        <Notice tone="warning" title="Some things could not be written as .kairo">
          <ul className="plan-item" style={{ padding: 0 }}>
            {exported.notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        </Notice>
      )}

      {loading && !exported && <Loading label="Reading the structure…" />}

      {exported && exported.tableCount === 0 && (
        <Notice tone="info" title="This database has no tables to export">
          <p className="notice-hint">Views are not exported. Create a table first.</p>
        </Notice>
      )}

      {exported && exported.tableCount > 0 && (
        <Window
          className="fill"
          title={`${suggested} · ${plural(exported.tableCount, "table")}`}
          actions={
            <>
              <CopyButton text={exported.text} />
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => app.requestSchema({ text: exported.text, name: suggested })}
              >
                Open in Schema workspace
              </button>
            </>
          }
        >
          <div className="window-body">
            <CodeBlock code={exported.text} language="kairo" />
          </div>
        </Window>
      )}

      <CliHint command={`kairo export ${cliTarget} -o schema/${suggested}`} />
    </div>
  );
}
