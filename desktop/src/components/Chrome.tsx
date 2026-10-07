import { useState } from "react";
import { formatBytes, formatClock } from "../lib/format";
import { useApp, VIEWS } from "../state/app";

/** The strip above every page: which page, and which database it is acting on. */
export function PageHeader() {
  const { view, active } = useApp();
  const title = VIEWS.find((v) => v.id === view)?.label ?? "";

  return (
    <header className="page-header">
      <h1>{title}</h1>
      <div className="page-context" aria-label="Current database">
        <span className="divider" aria-hidden="true" />
        {active ? (
          <>
            <span className="status-dot on" role="img" aria-label="Connected" />
            <strong className="truncate">{active.name}</strong>
            <span className="badge badge-accent">
              {active.engine === "sqlite" ? "SQLite" : "PostgreSQL"}
            </span>
            <span className="location mono truncate selectable" title={active.location}>
              {active.location}
            </span>
            {active.readOnly && <span className="badge badge-amber">read-only</span>}
            {active.sizeBytes !== null && <span>{formatBytes(active.sizeBytes)}</span>}
            <span>{active.serverVersion}</span>
          </>
        ) : (
          <>
            <span className="status-dot" role="img" aria-label="Not connected" />
            <span>No database open</span>
          </>
        )}
      </div>
    </header>
  );
}

/** The bottom strip: the latest event, and the activity log behind it. */
export function StatusBar() {
  const { activity, info } = useApp();
  const [open, setOpen] = useState(false);
  const latest = activity[0];

  return (
    <footer>
      {open && (
        <section className="activity" id="activity-log" aria-label="Activity">
          {activity.length === 0 ? (
            <p className="loading-row">Nothing has happened yet in this session.</p>
          ) : (
            <ol>
              {activity.map((event) => (
                <li key={event.id} className="activity-item">
                  <time dateTime={event.at.toISOString()}>{formatClock(event.at)}</time>
                  <span className={`activity-mark ${event.level}`} aria-hidden="true" />
                  <div className="selectable">
                    <span className="visually-hidden">{event.level}: </span>
                    {event.title}
                    {event.detail && <div className="activity-detail">{event.detail}</div>}
                  </div>
                </li>
              ))}
            </ol>
          )}
        </section>
      )}
      <div className="statusbar">
        <button
          type="button"
          aria-expanded={open}
          aria-controls="activity-log"
          onClick={() => setOpen((value) => !value)}
        >
          Activity{activity.length > 0 ? ` (${activity.length})` : ""}
        </button>
        {/* Announced politely, so a screen reader hears each outcome. */}
        <span
          className={latest ? `truncate level-${latest.level}` : "truncate"}
          role="status"
          aria-live="polite"
        >
          {latest ? latest.title : "Ready"}
        </span>
        <span className="spacer" />
        <span>Local-first. Nothing leaves this machine.</span>
        {info && <span className="mono">kairo {info.version}</span>}
      </div>
    </footer>
  );
}
