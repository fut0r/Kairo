import { pickDatabaseFile } from "../api/dialogs";
import type { RecentItem } from "../api/types";
import { formatAgo } from "../lib/format";
import { useApp, VIEWS } from "../state/app";
import { Icon, LogoMark, type IconName } from "./Icon";

const RECENT_ICONS: Record<RecentItem["kind"], IconName> = {
  sqlite: "file",
  postgres: "plug",
  project: "folder",
};

export function Sidebar() {
  const app = useApp();
  const modifier = app.info?.platform === "macos" ? "⌘" : "Ctrl+";

  const openDatabase = async () => {
    try {
      const path = await pickDatabaseFile();
      if (!path) return;
      await app.connect({ kind: "sqlite", path });
      if (app.view === "overview") app.navigate("explorer");
    } catch (error) {
      app.logError("Could not open the database", error);
      // Show the reason where the user can act on it.
      app.openConnectDialog({ tab: "sqlite" });
    }
  };

  // Connections that are open are listed above; do not repeat them here.
  const openKeys = new Set(app.connections.map((c) => c.workspaceKey));
  const recents = app.recents.filter((item) => !openKeys.has(item.key));

  return (
    <nav className="sidebar" aria-label="Workspace">
      <div className="sidebar-brand">
        <LogoMark />
        <span className="brand-sep" aria-hidden="true">
          |
        </span>
        <span className="brand-name">
          Kairo<span className="brand-db">DB</span>
        </span>
        {app.info && <span className="brand-version">v{app.info.version}</span>}
      </div>

      <div className="sidebar-actions">
        <button type="button" className="btn btn-primary btn-block" onClick={openDatabase}>
          <Icon name="file" size={14} />
          Open Database
        </button>
        <button
          type="button"
          className="btn btn-outline btn-block"
          onClick={() => app.openConnectDialog({ tab: "postgres" })}
        >
          <Icon name="plug" size={14} />
          New Connection
        </button>
      </div>

      <div className="sidebar-scroll">
        <section className="sidebar-section" aria-labelledby="sidebar-connections">
          <h2 className="sidebar-heading" id="sidebar-connections">
            Connections
          </h2>
          {app.connections.length === 0 ? (
            <p className="sidebar-empty">Nothing open yet.</p>
          ) : (
            <ul>
              {app.connections.map((connection) => (
                <li key={connection.id} className="nav-row">
                  <button
                    type="button"
                    className={connection.id === app.active?.id ? "nav-item active" : "nav-item"}
                    aria-current={connection.id === app.active?.id ? "true" : undefined}
                    onClick={() => app.setActive(connection.id)}
                    title={connection.location}
                  >
                    <span className="status-dot on" aria-hidden="true" />
                    <span className="nav-text truncate">
                      {connection.name}
                      <span className="nav-sub truncate">
                        {connection.engine === "sqlite" ? "SQLite" : (connection.host ?? "PostgreSQL")}
                      </span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={`Close ${connection.name}`}
                    title="Close connection"
                    onClick={() => void app.disconnect(connection.id)}
                  >
                    <Icon name="close" size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="sidebar-section" aria-labelledby="sidebar-navigate">
          <h2 className="sidebar-heading" id="sidebar-navigate">
            Navigate
          </h2>
          <ul>
            {VIEWS.map((view, index) => (
              <li key={view.id}>
                <button
                  type="button"
                  className={view.id === app.view ? "nav-item active" : "nav-item"}
                  aria-current={view.id === app.view ? "page" : undefined}
                  onClick={() => app.navigate(view.id)}
                >
                  <span className="nav-text">{view.label}</span>
                  <span className="nav-key" aria-hidden="true">
                    {modifier}
                    {index + 1}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>

        <section className="sidebar-section" aria-labelledby="sidebar-recent">
          <h2 className="sidebar-heading" id="sidebar-recent">
            Recent
          </h2>
          {recents.length === 0 ? (
            <p className="sidebar-empty">Databases and projects you open appear here.</p>
          ) : (
            <ul>
              {recents.map((item) => (
                <li key={item.key} className="nav-row">
                  <button
                    type="button"
                    className="nav-item"
                    onClick={() => void app.reopenRecent(item)}
                    title={`${item.location}\nOpened ${formatAgo(item.lastOpened)}`}
                  >
                    <Icon name={RECENT_ICONS[item.kind]} size={14} />
                    <span className="nav-text truncate">
                      {item.name}
                      <span className="nav-sub truncate">{item.location}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={`Remove ${item.name} from recent`}
                    title="Remove from recent"
                    onClick={() => void app.forgetRecent(item.key)}
                  >
                    <Icon name="close" size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </nav>
  );
}
