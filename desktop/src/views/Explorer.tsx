import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type KairoError } from "../api/client";
import type { ColumnInfo, RowPage, SortSpec, TableDetail, TableSummary } from "../api/types";
import { CodeBlock, CopyButton, Tabs, Window, tabPanelProps } from "../components/Controls";
import { DataGrid } from "../components/DataGrid";
import { Icon } from "../components/Icon";
import { ErrorNotice, Loading } from "../components/Notice";
import { toKairoError } from "../lib/errors";
import { formatCount, pageRange } from "../lib/format";
import { useApp } from "../state/app";
import { NoDatabase } from "./NoDatabase";

type DetailTab = "data" | "structure" | "indexes" | "sql";

const PAGE_SIZES = [25, 50, 100, 250, 500];
const FILTER_DELAY_MS = 300;

export function Explorer() {
  const app = useApp();
  const active = app.active;
  const [tables, setTables] = useState<TableSummary[] | null>(null);
  const [listError, setListError] = useState<KairoError | null>(null);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [details, setDetails] = useState<Record<string, TableDetail>>({});
  const [reload, setReload] = useState(0);

  const id = active?.id;
  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    setListError(null);
    api
      .listTables(id)
      .then((result) => {
        if (cancelled) return;
        setTables(result);
        // Structure may have changed; drop what was cached.
        setDetails({});
        setSelected((current) =>
          current && result.some((t) => t.name === current) ? current : (result[0]?.name ?? null),
        );
      })
      .catch((raw) => !cancelled && setListError(toKairoError(raw)));
    return () => {
      cancelled = true;
    };
  }, [id, app.catalogVersion, reload]);

  // Another view asked for a specific table.
  const requested = app.tableRequest;
  useEffect(() => {
    if (requested) setSelected(requested.name);
  }, [requested]);

  // `logError` is stable; depending on the whole context would recreate
  // this callback on every state change and refetch in a loop.
  const { logError } = app;
  const loadDetail = useCallback(
    async (table: string) => {
      if (!id) return;
      try {
        const detail = await api.describeTable(id, table);
        setDetails((current) => ({ ...current, [table]: detail }));
      } catch (raw) {
        logError(`Could not read ${table}`, raw);
      }
    },
    [id, logError],
  );

  const toggle = (table: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(table)) next.delete(table);
      else next.add(table);
      return next;
    });
  };

  const visible = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (tables ?? []).filter((t) => t.name.toLowerCase().includes(needle));
  }, [tables, search]);

  if (!active) {
    return <NoDatabase action="browse its tables" />;
  }

  const summary = tables?.find((t) => t.name === selected) ?? null;

  return (
    <div className="split" style={{ gridTemplateColumns: "280px minmax(0, 1fr)" }}>
      <aside className="pane" aria-label="Tables">
        <div className="toolbar">
          <label className="visually-hidden" htmlFor="table-search">
            Find a table
          </label>
          <input
            id="table-search"
            className="input input-sm"
            type="search"
            placeholder="Find a table"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
          <button
            type="button"
            className="icon-btn"
            aria-label="Reload tables"
            title="Reload tables"
            onClick={() => setReload((n) => n + 1)}
          >
            <Icon name="refresh" size={14} />
          </button>
        </div>

        <div className="pane-scroll">
          {listError && (
            <div className="pane-pad">
              <ErrorNotice error={listError} />
            </div>
          )}
          {!tables && !listError && <Loading label="Reading tables…" />}
          {tables && tables.length === 0 && (
            <p className="pane-pad muted">
              This database has no tables. Apply a schema or create one with SQL.
            </p>
          )}
          {tables && tables.length > 0 && visible.length === 0 && (
            <p className="pane-pad muted">No table matches “{search}”.</p>
          )}

          <ul>
            {visible.map((table) => {
              const open = expanded.has(table.name);
              const detail = details[table.name];
              return (
                <li key={table.name}>
                  <div className={table.name === selected ? "tree-row active" : "tree-row"}>
                    <button
                      type="button"
                      className="tree-toggle"
                      aria-expanded={open}
                      aria-label={`${open ? "Hide" : "Show"} columns of ${table.name}`}
                      onClick={() => toggle(table.name)}
                    >
                      <Icon name="chevron" size={12} />
                    </button>
                    <button
                      type="button"
                      className="tree-main"
                      aria-current={table.name === selected ? "true" : undefined}
                      onClick={() => setSelected(table.name)}
                    >
                      <Icon name={table.kind === "view" ? "view" : "table"} size={14} />
                      <span className="truncate">{table.name}</span>
                      <span className="tree-count">
                        {table.rowCount === null
                          ? ""
                          : `${table.rowCountEstimated ? "~" : ""}${formatCount(table.rowCount)}`}
                      </span>
                    </button>
                  </div>
                  {open && <TreeColumns table={table.name} detail={detail} loadDetail={loadDetail} />}
                </li>
              );
            })}
          </ul>
        </div>
      </aside>

      <section className="pane" aria-label="Table detail">
        {selected && summary ? (
          <TablePanel
            key={`${selected}:${app.catalogVersion}:${reload}`}
            connectionId={active.id}
            summary={summary}
            detail={details[selected]}
            loadDetail={loadDetail}
          />
        ) : (
          <div className="empty">
            <h2>Select a table</h2>
            <p>Its rows, columns, indexes and SQL appear here.</p>
          </div>
        )}
      </section>
    </div>
  );
}

/** The columns under an expanded table. Loads them when they are not cached. */
function TreeColumns({
  table,
  detail,
  loadDetail,
}: {
  table: string;
  detail: TableDetail | undefined;
  loadDetail: (table: string) => Promise<void>;
}) {
  useEffect(() => {
    if (!detail) void loadDetail(table);
  }, [detail, loadDetail, table]);

  return (
    <ul className="tree-children" aria-label={`Columns of ${table}`}>
      {!detail && (
        <li className="tree-child">
          <span className="spinner" aria-hidden="true" /> Loading…
        </li>
      )}
      {detail?.columns.map((column) => (
        <li key={column.name} className="tree-child">
          <span className="name" title={column.name}>
            {column.name}
          </span>
          {column.primaryKeyPosition > 0 && <span className="badge badge-accent">PK</span>}
          <TypeBadge column={column} />
        </li>
      ))}
    </ul>
  );
}

/** The column's own type, coloured by the kind of data it holds. */
function TypeBadge({ column }: { column: ColumnInfo }) {
  const tone: Record<string, string> = {
    int: "badge-accent",
    float: "badge-accent",
    bool: "badge-amber",
    string: "badge-green",
    timestamp: "badge-amber",
    blob: "",
  };
  return (
    <span
      className={`badge ${tone[column.kairoType] ?? ""}`}
      title={`${column.dataType || "no declared type"} · .kairo type: ${column.kairoType}`}
    >
      {(column.dataType || column.kairoType).toLowerCase()}
    </span>
  );
}

interface TablePanelProps {
  connectionId: string;
  summary: TableSummary;
  detail: TableDetail | undefined;
  loadDetail: (table: string) => Promise<void>;
}

function TablePanel({ connectionId, summary, detail, loadDetail }: TablePanelProps) {
  const [tab, setTab] = useState<DetailTab>("data");
  const table = summary.name;

  useEffect(() => {
    if (!detail) void loadDetail(table);
  }, [detail, loadDetail, table]);

  return (
    <>
      <div className="toolbar">
        <Icon name={summary.kind === "view" ? "view" : "table"} />
        <h2 className="truncate">{table}</h2>
        {summary.kind === "view" && <span className="badge">view</span>}
        <span className="muted">
          {summary.columnCount} column{summary.columnCount === 1 ? "" : "s"}
        </span>
      </div>

      <Tabs
        label={`Sections of ${table}`}
        idPrefix="table"
        value={tab}
        onChange={setTab}
        items={[
          { id: "data", label: "Data" },
          { id: "structure", label: "Structure", count: detail?.columns.length },
          {
            id: "indexes",
            label: "Indexes",
            count: detail ? detail.indexes.length + detail.foreignKeys.length : undefined,
          },
          { id: "sql", label: "SQL" },
        ]}
      />

      <div className="fill" {...tabPanelProps("table", tab)}>
        {tab === "data" && <DataTab connectionId={connectionId} table={table} />}
        {tab !== "data" && !detail && <Loading label="Reading structure…" />}
        {tab === "structure" && detail && <StructureTab detail={detail} />}
        {tab === "indexes" && detail && <IndexesTab detail={detail} />}
        {tab === "sql" && detail && (
          <div className="pane-pad fill">
            <Window
              title={`${detail.kind} ${detail.name}`}
              actions={<CopyButton text={detail.createSql} label="Copy SQL" />}
              className="fill"
            >
              <div className="window-body">
                <CodeBlock code={detail.createSql || "-- No definition is stored for this object."} language="sql" />
              </div>
            </Window>
          </div>
        )}
      </div>
    </>
  );
}

function DataTab({ connectionId, table }: { connectionId: string; table: string }) {
  const { settings, requestQuery } = useApp();
  const [pageSize, setPageSize] = useState(settings.pageSize);
  const [offset, setOffset] = useState(0);
  const [filterInput, setFilterInput] = useState("");
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<SortSpec | null>(null);
  const [page, setPage] = useState<RowPage | null>(null);
  const [error, setError] = useState<KairoError | null>(null);
  const [loading, setLoading] = useState(true);
  const [reload, setReload] = useState(0);

  // Wait for typing to pause before filtering, and go back to the first page.
  // Only when the filter really changes: without that check the timer also
  // ran once on opening a table and undid a page change made in its first
  // 300 ms.
  useEffect(() => {
    const next = filterInput.trim();
    if (next === filter) return;
    const timer = window.setTimeout(() => {
      setFilter(next);
      setOffset(0);
    }, FILTER_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [filterInput, filter]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .fetchRows(connectionId, table, { limit: pageSize, offset, filter: filter || null, sort })
      .then((result) => {
        if (cancelled) return;
        setPage(result);
        setError(null);
      })
      .catch((raw) => !cancelled && setError(toKairoError(raw)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [connectionId, table, pageSize, offset, filter, sort, reload]);

  // First click sorts ascending, second descending, third clears.
  const onSort = (column: string) => {
    setOffset(0);
    setSort((current) => {
      if (current?.column !== column) return { column, descending: false };
      return current.descending ? null : { column, descending: true };
    });
  };

  const total = page?.totalRows ?? 0;
  const shown = page?.rows.length ?? 0;
  const lastOffset = total === 0 ? 0 : Math.floor((total - 1) / pageSize) * pageSize;
  const atStart = offset === 0;
  const atEnd = offset + shown >= total;

  return (
    <div className="fill">
      <div className="toolbar">
        <label className="visually-hidden" htmlFor="row-filter">
          Filter rows of {table}
        </label>
        <input
          id="row-filter"
          className="input input-sm"
          style={{ maxWidth: 260 }}
          type="search"
          placeholder="Filter rows (any column)"
          value={filterInput}
          onChange={(event) => setFilterInput(event.target.value)}
        />
        {loading && <span className="spinner" role="status" aria-label="Loading rows" />}
        <span className="spacer" />

        <span className="muted" aria-live="polite">
          {page
            ? `${pageRange(page.offset, shown)} of ${formatCount(total)}${filter ? " matching" : ""}`
            : ""}
        </span>

        <div className="row" role="group" aria-label="Pages">
          <button type="button" className="icon-btn" aria-label="First page" disabled={atStart} onClick={() => setOffset(0)}>
            <Icon name="first" size={14} />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label="Previous page"
            disabled={atStart}
            onClick={() => setOffset(Math.max(0, offset - pageSize))}
          >
            <Icon name="prev" size={14} />
          </button>
          <button
            type="button"
            className="icon-btn"
            aria-label="Next page"
            disabled={atEnd}
            onClick={() => setOffset(offset + pageSize)}
          >
            <Icon name="next" size={14} />
          </button>
          <button type="button" className="icon-btn" aria-label="Last page" disabled={atEnd} onClick={() => setOffset(lastOffset)}>
            <Icon name="last" size={14} />
          </button>
        </div>

        <label className="visually-hidden" htmlFor="page-size">
          Rows per page
        </label>
        <select
          id="page-size"
          className="select select-sm"
          value={pageSize}
          onChange={(event) => {
            setPageSize(Number(event.target.value));
            setOffset(0);
          }}
        >
          {[...new Set([...PAGE_SIZES, settings.pageSize])]
            .sort((a, b) => a - b)
            .map((size) => (
              <option key={size} value={size}>
                {size} rows
              </option>
            ))}
        </select>

        <button type="button" className="icon-btn" aria-label="Reload rows" title="Reload rows" onClick={() => setReload((n) => n + 1)}>
          <Icon name="refresh" size={14} />
        </button>
      </div>

      <div className="fill pane-pad" style={{ gap: "var(--space-3)" }}>
        {error && <ErrorNotice error={error} />}
        {!page && !error && <Loading label="Loading rows…" />}

        {page && page.rows.length === 0 && !error && (
          <div className="empty">
            <h2>{filter ? "No rows match" : "This table is empty"}</h2>
            <p>
              {filter
                ? `Nothing in any column contains “${filter}”.`
                : "Rows you insert will show up here."}
            </p>
            {filter && (
              <div className="empty-actions">
                <button type="button" className="btn btn-outline btn-sm" onClick={() => setFilterInput("")}>
                  Clear filter
                </button>
              </div>
            )}
          </div>
        )}

        {page && page.rows.length > 0 && (
          <DataGrid
            label={`Rows of ${table}`}
            columns={page.columns}
            rows={page.rows}
            rowOffset={page.offset}
            sort={sort}
            onSort={onSort}
          />
        )}

        {page && (
          <div className="row">
            <span className="hint">Ran:</span>
            <code className="code-chip truncate" title={page.sql}>
              {page.sql}
            </code>
            <span className="spacer" />
            <CopyButton text={page.sql} label="Copy" />
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => requestQuery(`SELECT * FROM "${table.replace(/"/g, '""')}" LIMIT ${pageSize}`)}
            >
              Open in Query
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function StructureTab({ detail }: { detail: TableDetail }) {
  const references = (column: string) =>
    detail.foreignKeys
      .filter((key) => key.columns.includes(column))
      .map((key) => {
        const index = key.columns.indexOf(column);
        const target = key.referencesColumns[index];
        return target ? `${key.referencesTable}.${target}` : key.referencesTable;
      });

  return (
    <div className="pane-scroll">
      <table className="table">
        <thead>
          <tr>
            <th className="num">#</th>
            <th>Column</th>
            <th>Type</th>
            <th>Nullable</th>
            <th>Default</th>
            <th>Key</th>
          </tr>
        </thead>
        <tbody>
          {detail.columns.map((column, index) => {
            const refs = references(column.name);
            return (
              <tr key={column.name}>
                <td className="num muted">{index + 1}</td>
                <td className="mono">{column.name}</td>
                <td>
                  <div className="row row-wrap">
                    <TypeBadge column={column} />
                    <span className="muted mono" title="Closest .kairo type">
                      {column.kairoType}
                    </span>
                  </div>
                </td>
                <td>
                  {!column.nullable ? (
                    <span className="badge badge-amber">not null</span>
                  ) : column.primaryKeyPosition > 0 ? (
                    // SQLite reports key columns as nullable even though an
                    // integer key can never hold NULL. Saying "nullable"
                    // beside "primary key" would mislead.
                    <span className="muted" title="Part of the primary key">
                      key
                    </span>
                  ) : (
                    <span className="muted">nullable</span>
                  )}
                </td>
                <td className="mono">
                  {column.defaultValue ?? <span className="muted">none</span>}
                </td>
                <td>
                  <div className="row row-wrap">
                    {column.primaryKeyPosition > 0 && (
                      <span className="badge badge-accent">
                        primary key
                        {detail.columns.filter((c) => c.primaryKeyPosition > 0).length > 1
                          ? ` ${column.primaryKeyPosition}`
                          : ""}
                      </span>
                    )}
                    {refs.map((ref) => (
                      <span key={ref} className="badge" title="Foreign key">
                        → {ref}
                      </span>
                    ))}
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function IndexesTab({ detail }: { detail: TableDetail }) {
  if (detail.indexes.length === 0 && detail.foreignKeys.length === 0) {
    return (
      <div className="empty">
        <h2>No indexes or foreign keys</h2>
        <p>
          {detail.kind === "view"
            ? "Views do not have their own indexes."
            : "Lookups on this table scan every row unless they use the primary key."}
        </p>
      </div>
    );
  }

  return (
    <div className="pane-scroll stack" style={{ gap: 0 }}>
      {detail.indexes.length > 0 && (
        <table className="table">
          <caption className="visually-hidden">Indexes</caption>
          <thead>
            <tr>
              <th>Index</th>
              <th>Columns</th>
              <th>Unique</th>
              <th>From</th>
            </tr>
          </thead>
          <tbody>
            {detail.indexes.map((index) => (
              <tr key={index.name}>
                <td className="mono">{index.name}</td>
                <td className="mono">{index.columns.join(", ")}</td>
                <td>
                  {index.unique ? (
                    <span className="badge badge-accent">unique</span>
                  ) : (
                    <span className="muted">no</span>
                  )}
                </td>
                <td className="muted">{index.origin}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {detail.foreignKeys.length > 0 && (
        <table className="table">
          <caption className="visually-hidden">Foreign keys</caption>
          <thead>
            <tr>
              <th>Foreign key</th>
              <th>References</th>
              <th>On update</th>
              <th>On delete</th>
            </tr>
          </thead>
          <tbody>
            {detail.foreignKeys.map((key, index) => (
              <tr key={`${key.name ?? "fk"}-${index}`}>
                <td className="mono">{key.columns.join(", ")}</td>
                <td className="mono">
                  {key.referencesTable}
                  {key.referencesColumns.length > 0 ? ` (${key.referencesColumns.join(", ")})` : ""}
                </td>
                <td className="muted">{key.onUpdate ?? "—"}</td>
                <td className="muted">{key.onDelete ?? "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
