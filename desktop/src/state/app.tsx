import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { api } from "../api/client";
import type {
  AppInfo,
  ConnectRequest,
  ConnectionInfo,
  ProjectStatus,
  RecentItem,
  Settings,
} from "../api/types";
import { errorSummary, toKairoError } from "../lib/errors";

export type View = "overview" | "explorer" | "query" | "schema" | "export" | "settings";

export const VIEWS: { id: View; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "explorer", label: "Explorer" },
  { id: "query", label: "Query" },
  { id: "schema", label: "Schema" },
  { id: "export", label: "Export" },
  { id: "settings", label: "Settings" },
];

export type ActivityLevel = "success" | "error" | "info";

export interface ActivityEvent {
  id: number;
  at: Date;
  level: ActivityLevel;
  title: string;
  detail?: string;
}

export interface ConnectDialogState {
  tab: "sqlite" | "postgres";
  /** Prefills the PostgreSQL URL, for reconnecting to a saved connection. */
  url?: string;
}

/** A request from one view for another to load something. */
export interface SchemaRequest {
  nonce: number;
  path?: string;
  text?: string;
  name?: string;
}

export interface QueryRequest {
  nonce: number;
  sql: string;
}

export interface TableRequest {
  nonce: number;
  name: string;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: "dark",
  pageSize: 50,
  maxRows: 1000,
  queryTimeoutSecs: 30,
  confirmWrites: true,
};

const MAX_ACTIVITY = 200;

export interface AppContextValue {
  ready: boolean;
  info: AppInfo | null;
  settings: Settings;
  connections: ConnectionInfo[];
  active: ConnectionInfo | null;
  recents: RecentItem[];
  view: View;
  activity: ActivityEvent[];
  project: ProjectStatus | null;
  connectDialog: ConnectDialogState | null;
  schemaRequest: SchemaRequest | null;
  queryRequest: QueryRequest | null;
  tableRequest: TableRequest | null;
  /** Changes whenever the active database's tables may have changed. */
  catalogVersion: number;

  navigate: (view: View) => void;
  log: (level: ActivityLevel, title: string, detail?: string) => void;
  logError: (context: string, error: unknown) => void;
  connect: (request: ConnectRequest) => Promise<ConnectionInfo>;
  disconnect: (id: string) => Promise<void>;
  setActive: (id: string) => void;
  reopenRecent: (item: RecentItem) => Promise<void>;
  forgetRecent: (key: string) => Promise<void>;
  clearRecents: () => Promise<void>;
  openProject: (dir: string) => Promise<void>;
  initProject: (dir: string) => Promise<void>;
  closeProject: () => void;
  updateSettings: (patch: Partial<Settings>) => Promise<void>;
  openConnectDialog: (state: ConnectDialogState) => void;
  closeConnectDialog: () => void;
  requestSchema: (request: Omit<SchemaRequest, "nonce">) => void;
  requestQuery: (sql: string) => void;
  /** Opens a table in the Explorer. */
  requestTable: (name: string) => void;
  catalogChanged: () => void;
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const value = useContext(AppContext);
  if (!value) throw new Error("useApp must be used inside <AppProvider>");
  return value;
}

function resolveTheme(theme: Settings["theme"]): "dark" | "light" {
  if (theme !== "system") return theme;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [connections, setConnections] = useState<ConnectionInfo[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [recents, setRecents] = useState<RecentItem[]>([]);
  const [view, setView] = useState<View>("overview");
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const [project, setProject] = useState<ProjectStatus | null>(null);
  const [connectDialog, setConnectDialog] = useState<ConnectDialogState | null>(null);
  const [schemaRequest, setSchemaRequest] = useState<SchemaRequest | null>(null);
  const [queryRequest, setQueryRequest] = useState<QueryRequest | null>(null);
  const [tableRequest, setTableRequest] = useState<TableRequest | null>(null);
  const [catalogVersion, setCatalogVersion] = useState(0);
  const counter = useRef(0);

  const log = useCallback((level: ActivityLevel, title: string, detail?: string) => {
    counter.current += 1;
    const event: ActivityEvent = { id: counter.current, at: new Date(), level, title, detail };
    setActivity((events) => [event, ...events].slice(0, MAX_ACTIVITY));
  }, []);

  const logError = useCallback(
    (context: string, error: unknown) => {
      const failure = toKairoError(error);
      log("error", `${context}: ${failure.message}`, failure.detail ?? failure.hint);
    },
    [log],
  );

  // Load what the Rust side already knows. Connections survive a reload of
  // the webview because they live there, not here.
  useEffect(() => {
    let cancelled = false;
    Promise.all([api.appInfo(), api.getSettings(), api.listRecents(), api.listConnections()])
      .then(([appInfo, storedSettings, storedRecents, open]) => {
        if (cancelled) return;
        setInfo(appInfo);
        setSettings(storedSettings);
        setRecents(storedRecents);
        setConnections(open);
        setActiveId(open[open.length - 1]?.id ?? null);
      })
      .catch((error) => {
        if (!cancelled) log("error", errorSummary(error));
      })
      .finally(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [log]);

  useEffect(() => {
    const apply = () => {
      document.documentElement.dataset.theme = resolveTheme(settings.theme);
    };
    apply();
    if (settings.theme !== "system" || !window.matchMedia) return;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings.theme]);

  // After anything that may have written to a database, re-read what the Rust
  // side knows about the open connections (a SQLite file's size, for one).
  useEffect(() => {
    if (catalogVersion === 0) return;
    let cancelled = false;
    api
      .listConnections()
      .then((open) => !cancelled && setConnections(open))
      .catch(() => {
        // The header keeps its last known values.
      });
    return () => {
      cancelled = true;
    };
  }, [catalogVersion]);

  const refreshRecents = useCallback(async () => {
    try {
      setRecents(await api.listRecents());
    } catch {
      // The list is a convenience; a stale one is not worth an error.
    }
  }, []);

  const adopt = useCallback(
    (opened: ConnectionInfo) => {
      setConnections((current) => [
        // Reconnecting to the same database replaces its earlier session.
        ...current.filter((c) => c.workspaceKey !== opened.workspaceKey),
        opened,
      ]);
      setActiveId(opened.id);
      setCatalogVersion((v) => v + 1);
      log("success", `Connected to ${opened.name}`, `${opened.serverVersion} · ${opened.location}`);
      void refreshRecents();
    },
    [log, refreshRecents],
  );

  const connect = useCallback(
    async (request: ConnectRequest) => {
      const opened = await api.connect(request);
      adopt(opened);
      return opened;
    },
    [adopt],
  );

  const disconnect = useCallback(
    async (id: string) => {
      const closing = connections.find((c) => c.id === id);
      try {
        await api.disconnect(id);
      } catch (error) {
        logError("Disconnect failed", error);
      }
      const remaining = connections.filter((c) => c.id !== id);
      setConnections(remaining);
      setActiveId((current) =>
        current === id ? (remaining[remaining.length - 1]?.id ?? null) : current,
      );
      if (closing) log("info", `Closed ${closing.name}`);
    },
    [connections, log, logError],
  );

  const openProject = useCallback(
    async (dir: string) => {
      const status = await api.projectStatus(dir);
      setProject(status);
      void refreshRecents();
      if (!status.initialized) {
        log("info", "That folder is not a Kairo project yet", status.problem ?? status.root);
        return;
      }
      log("info", `Opened project ${status.root}`);
      try {
        adopt(await api.connectProject(dir));
      } catch (error) {
        logError("The project database could not be opened", error);
      }
    },
    [adopt, log, logError, refreshRecents],
  );

  const initProject = useCallback(
    async (dir: string) => {
      const status = await api.initProject(dir);
      setProject(status);
      log("success", "Project initialised", status.root);
      void refreshRecents();
    },
    [log, refreshRecents],
  );

  const reopenRecent = useCallback(
    async (item: RecentItem) => {
      try {
        if (item.kind === "sqlite") {
          await connect({ kind: "sqlite", path: item.target });
          setView((current) => (current === "overview" ? "explorer" : current));
        } else if (item.kind === "postgres") {
          // The password was never stored, so it has to be asked for again.
          setConnectDialog({ tab: "postgres", url: item.target });
        } else {
          await openProject(item.target);
        }
      } catch (error) {
        logError(`Could not open ${item.name}`, error);
      }
    },
    [connect, logError, openProject],
  );

  const forgetRecent = useCallback(
    async (key: string) => {
      try {
        setRecents(await api.removeRecent(key));
      } catch (error) {
        logError("Could not update recents", error);
      }
    },
    [logError],
  );

  const clearRecents = useCallback(async () => {
    try {
      await api.clearRecents();
      setRecents([]);
      log("info", "Recent connections and query history cleared");
    } catch (error) {
      logError("Could not clear recents", error);
    }
  }, [log, logError]);

  const updateSettings = useCallback(
    async (patch: Partial<Settings>) => {
      const next = { ...settings, ...patch };
      setSettings(next);
      try {
        setSettings(await api.setSettings(next));
      } catch (error) {
        logError("Settings were not saved", error);
      }
    },
    [logError, settings],
  );

  const requestSchema = useCallback((request: Omit<SchemaRequest, "nonce">) => {
    setSchemaRequest({ ...request, nonce: Date.now() + Math.random() });
    setView("schema");
  }, []);

  const requestQuery = useCallback((sql: string) => {
    setQueryRequest({ sql, nonce: Date.now() + Math.random() });
    setView("query");
  }, []);

  const requestTable = useCallback((name: string) => {
    setTableRequest({ name, nonce: Date.now() + Math.random() });
    setView("explorer");
  }, []);

  const value = useMemo<AppContextValue>(
    () => ({
      ready,
      info,
      settings,
      connections,
      active: connections.find((c) => c.id === activeId) ?? null,
      recents,
      view,
      activity,
      project,
      connectDialog,
      schemaRequest,
      queryRequest,
      tableRequest,
      catalogVersion,
      navigate: setView,
      log,
      logError,
      connect,
      disconnect,
      setActive: setActiveId,
      reopenRecent,
      forgetRecent,
      clearRecents,
      openProject,
      initProject,
      closeProject: () => setProject(null),
      updateSettings,
      openConnectDialog: setConnectDialog,
      closeConnectDialog: () => setConnectDialog(null),
      requestSchema,
      requestQuery,
      requestTable,
      catalogChanged: () => setCatalogVersion((v) => v + 1),
    }),
    [
      ready,
      info,
      settings,
      connections,
      activeId,
      recents,
      view,
      activity,
      project,
      connectDialog,
      schemaRequest,
      queryRequest,
      tableRequest,
      catalogVersion,
      log,
      logError,
      connect,
      disconnect,
      reopenRecent,
      forgetRecent,
      clearRecents,
      openProject,
      initProject,
      updateSettings,
      requestSchema,
      requestQuery,
      requestTable,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}
