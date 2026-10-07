import { useEffect, useState, type ReactNode } from "react";
import { pickDatabaseFile } from "./api/dialogs";
import { PageHeader, StatusBar } from "./components/Chrome";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { Loading } from "./components/Notice";
import { Sidebar } from "./components/Sidebar";
import { AppProvider, useApp, VIEWS, type View } from "./state/app";
import { Explorer } from "./views/Explorer";
import { ExportView } from "./views/ExportView";
import { Overview } from "./views/Overview";
import { QueryView } from "./views/QueryView";
import { SchemaView } from "./views/SchemaView";
import { SettingsView } from "./views/SettingsView";

const SCREENS: Record<View, () => ReactNode> = {
  overview: () => <Overview />,
  explorer: () => <Explorer />,
  query: () => <QueryView />,
  schema: () => <SchemaView />,
  export: () => <ExportView />,
  settings: () => <SettingsView />,
};

function Shell() {
  const app = useApp();
  const { view, navigate, connect, logError, openConnectDialog } = app;
  // A view is created the first time it is shown and then kept, so an
  // unsaved schema or a half-written query survives switching pages.
  const [visited, setVisited] = useState<Set<View>>(new Set([view]));

  useEffect(() => {
    setVisited((current) => (current.has(view) ? current : new Set(current).add(view)));
  }, [view]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;

      const target = VIEWS[Number(event.key) - 1];
      if (target) {
        event.preventDefault();
        navigate(target.id);
        return;
      }

      if (event.key.toLowerCase() === "o") {
        event.preventDefault();
        pickDatabaseFile()
          .then((path) => (path ? connect({ kind: "sqlite", path }) : null))
          .catch((error) => {
            logError("Could not open the database", error);
            openConnectDialog({ tab: "sqlite" });
          });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, connect, logError, openConnectDialog]);

  return (
    <div className="app">
      <Sidebar />
      <div className="main">
        <PageHeader />
        <main className="page-body">
          {!app.ready ? (
            <Loading label="Starting…" />
          ) : (
            VIEWS.filter((screen) => visited.has(screen.id)).map((screen) => (
              <div key={screen.id} className="view" hidden={screen.id !== view}>
                {SCREENS[screen.id]()}
              </div>
            ))
          )}
        </main>
        <StatusBar />
      </div>
      <ConnectionDialog />
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
