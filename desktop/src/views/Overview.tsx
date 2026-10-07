import { useEffect, useState } from "react";
import { api, type KairoError } from "../api/client";
import { pickDatabaseFile, pickProjectFolder } from "../api/dialogs";
import type { TableSummary } from "../api/types";
import { CliHint } from "../components/Controls";
import { Icon } from "../components/Icon";
import { ErrorNotice, Loading, Notice } from "../components/Notice";
import { toKairoError } from "../lib/errors";
import { formatBytes, formatCount } from "../lib/format";
import { useApp } from "../state/app";

export function Overview() {
  const { active, project } = useApp();
  return (
    <div className="page-pad stack-lg">
      {active ? <DatabaseSummary /> : <Welcome />}
      {project && <ProjectCard />}
    </div>
  );
}

/** The first screen: what Kairo does and the three ways to start. */
function Welcome() {
  const app = useApp();

  const openFile = async () => {
    try {
      const path = await pickDatabaseFile();
      if (!path) return;
      await app.connect({ kind: "sqlite", path });
      app.navigate("explorer");
    } catch (error) {
      app.logError("Could not open the database", error);
      app.openConnectDialog({ tab: "sqlite" });
    }
  };

  const openFolder = async () => {
    try {
      const dir = await pickProjectFolder();
      if (dir) await app.openProject(dir);
    } catch (error) {
      app.logError("Could not open the project", error);
    }
  };

  return (
    <>
      <div className="empty" style={{ padding: 0 }}>
        <p className="eyebrow">KairoDB v{app.info?.version ?? "1.0.0"}</p>
        <h2 style={{ fontSize: 30, letterSpacing: "-0.5px" }}>Open a database to begin.</h2>
        <p>
          Browse tables and rows, run queries with a safety check, and apply schemas written in
          plain <code>.kairo</code> files. Everything runs on this machine.
        </p>
      </div>

      <div className="hairline-grid cols-3">
        <button type="button" className="card" onClick={openFile}>
          <h3>Open a SQLite file</h3>
          <p>Pick any .db file and get its tables, columns, indexes and rows.</p>
          <code className="code-chip">kairo read myapp.db</code>
        </button>
        <button
          type="button"
          className="card"
          onClick={() => app.openConnectDialog({ tab: "postgres" })}
        >
          <h3>Connect to PostgreSQL</h3>
          <p>Paste a connection URL. The password stays in memory and is never saved.</p>
          <code className="code-chip">kairo read postgres://…</code>
        </button>
        <button type="button" className="card" onClick={openFolder}>
          <h3>Open a project folder</h3>
          <p>A folder with a kairo.config and a schema directory, as kairo init creates.</p>
          <code className="code-chip">kairo status</code>
        </button>
      </div>

      <div className="row row-wrap">
        <span className="muted">No database yet?</span>
        <button type="button" className="btn btn-outline btn-sm" onClick={() => app.navigate("schema")}>
          Start with a schema
        </button>
      </div>
    </>
  );
}

function DatabaseSummary() {
  const app = useApp();
  const active = app.active;
  const [tables, setTables] = useState<TableSummary[] | null>(null);
  const [error, setError] = useState<KairoError | null>(null);

  const id = active?.id;
  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    setTables(null);
    setError(null);
    api
      .listTables(id)
      .then((result) => !cancelled && setTables(result))
      .catch((raw) => !cancelled && setError(toKairoError(raw)));
    return () => {
      cancelled = true;
    };
  }, [id, app.catalogVersion]);

  if (!active) return null;

  const real = tables?.filter((t) => t.kind === "table") ?? [];
  const views = tables?.filter((t) => t.kind === "view") ?? [];
  const counted = real.filter((t) => t.rowCount !== null);
  const totalRows = counted.reduce((sum, t) => sum + (t.rowCount ?? 0), 0);
  const estimated = counted.some((t) => t.rowCountEstimated);

  return (
    <>
      <div className="hairline-grid cols-4">
        <div className="stat">
          <div className="stat-label">Tables</div>
          <div className="stat-value">{tables ? formatCount(real.length) : "…"}</div>
        </div>
        <div className="stat">
          <div className="stat-label">Views</div>
          <div className="stat-value">{tables ? formatCount(views.length) : "…"}</div>
        </div>
        <div className="stat">
          <div className="stat-label">Rows{estimated ? " (estimated)" : ""}</div>
          <div className="stat-value">
            {tables ? (counted.length > 0 ? formatCount(totalRows) : "—") : "…"}
          </div>
        </div>
        <div className="stat">
          <div className="stat-label">{active.engine === "sqlite" ? "File size" : "Server"}</div>
          <div className="stat-value small">
            {active.engine === "sqlite"
              ? active.sizeBytes !== null
                ? formatBytes(active.sizeBytes)
                : "—"
              : active.serverVersion}
          </div>
        </div>
      </div>

      <section className="stack" aria-labelledby="overview-tables">
        <div className="row">
          <h2 id="overview-tables">Tables</h2>
          <span className="spacer" />
          <button type="button" className="btn btn-outline btn-sm" onClick={() => app.navigate("query")}>
            Write a query
          </button>
          <button type="button" className="btn btn-outline btn-sm" onClick={() => app.navigate("export")}>
            Export as .kairo
          </button>
        </div>

        {error && <ErrorNotice error={error} />}
        {!tables && !error && <Loading label="Reading the catalog…" />}

        {tables && tables.length === 0 && (
          <Notice tone="info" title="This database has no tables yet">
            <p className="notice-hint">
              Define some in the Schema workspace and apply them, or create them with SQL.
            </p>
            <div className="empty-actions">
              <button type="button" className="btn btn-primary btn-sm" onClick={() => app.navigate("schema")}>
                Open Schema workspace
              </button>
            </div>
          </Notice>
        )}

        {tables && tables.length > 0 && (
          <ul className="hairline-grid">
            {tables.map((table) => (
              <li key={table.name}>
                <button type="button" className="list-row" onClick={() => app.requestTable(table.name)}>
                  <Icon name={table.kind === "view" ? "view" : "table"} size={14} />
                  <span className="truncate">{table.name}</span>
                  {table.kind === "view" && <span className="badge">view</span>}
                  <span className="spacer" />
                  <span className="mono muted">
                    {table.columnCount} col{table.columnCount === 1 ? "" : "s"}
                  </span>
                  <span className="mono muted" style={{ minWidth: 96, textAlign: "end" }}>
                    {table.rowCount === null
                      ? "—"
                      : `${table.rowCountEstimated ? "~" : ""}${formatCount(table.rowCount)} rows`}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}

/** Status of an opened project folder, as `kairo status` reports it. */
function ProjectCard() {
  const app = useApp();
  const project = app.project;
  if (!project) return null;

  return (
    <section className="stack" aria-labelledby="overview-project">
      <div className="row">
        <h2 id="overview-project">Project</h2>
        <span className="spacer" />
        <button type="button" className="btn btn-ghost btn-sm" onClick={app.closeProject}>
          Close project
        </button>
      </div>

      {project.problem && (
        <Notice tone="error" title="kairo.config could not be used">
          <div className="notice-detail">{project.problem}</div>
        </Notice>
      )}

      {!project.initialized && !project.problem ? (
        <Notice
          tone="info"
          title="This folder is not a Kairo project yet"
          action={
            <button
              type="button"
              className="btn btn-primary btn-sm"
              onClick={() =>
                app.initProject(project.root).catch((error) => app.logError("Could not initialise", error))
              }
            >
              Initialise here
            </button>
          }
        >
          <p className="notice-hint">
            Initialising creates kairo.config and the schema, data, migrations, queries and plugins
            folders. Existing files are left alone.
          </p>
          <div className="notice-detail">{project.root}</div>
        </Notice>
      ) : (
        <div className="hairline-grid cols-2">
          <div className="card">
            <dl className="kv">
              <dt>Folder</dt>
              <dd className="mono">{project.root}</dd>
              <dt>Adapter</dt>
              <dd>{project.adapter ?? "—"}</dd>
              <dt>Database</dt>
              <dd className="mono">{project.database ?? "—"}</dd>
              {project.databaseExists !== null && (
                <>
                  <dt>Database file</dt>
                  <dd>{project.databaseExists ? "exists" : "not created yet"}</dd>
                </>
              )}
            </dl>
          </div>
          <div>
            <div className="list-row">
              <span className="section-title">Schemas ({project.schemaFiles.length})</span>
            </div>
            {project.schemaFiles.length === 0 ? (
              <div className="list-row muted">No .kairo files in schema/ yet.</div>
            ) : (
              project.schemaFiles.map((file) => (
                <button
                  key={file.path}
                  type="button"
                  className="list-row"
                  onClick={() => app.requestSchema({ path: file.path })}
                  title={`Open ${file.path} in the Schema workspace`}
                >
                  <Icon name="file" size={14} />
                  <span className="truncate">{file.name}.kairo</span>
                </button>
              ))
            )}
          </div>
        </div>
      )}

      <CliHint command="kairo status" />
    </section>
  );
}
