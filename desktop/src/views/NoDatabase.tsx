import { pickDatabaseFile } from "../api/dialogs";
import { useApp } from "../state/app";

/** Shown by views that need a database when none is open. */
export function NoDatabase({ action }: { action: string }) {
  const app = useApp();

  const openFile = async () => {
    try {
      const path = await pickDatabaseFile();
      if (path) await app.connect({ kind: "sqlite", path });
    } catch (error) {
      app.logError("Could not open the database", error);
      app.openConnectDialog({ tab: "sqlite" });
    }
  };

  return (
    <div className="empty">
      <p className="eyebrow">No database open</p>
      <h2>Open a database to {action}.</h2>
      <p>Pick a SQLite file, or connect to a PostgreSQL server with its URL.</p>
      <div className="empty-actions">
        <button type="button" className="btn btn-primary" onClick={openFile}>
          Open Database
        </button>
        <button
          type="button"
          className="btn btn-outline"
          onClick={() => app.openConnectDialog({ tab: "postgres" })}
        >
          New Connection
        </button>
      </div>
    </div>
  );
}
