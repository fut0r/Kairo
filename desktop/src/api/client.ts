import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  ApplyPlan,
  ApplyReport,
  ConnectRequest,
  ConnectionCheck,
  ConnectionInfo,
  ErrorKind,
  ExportedSchema,
  HistoryEntry,
  KairoErrorData,
  PageRequest,
  PreparedQuery,
  ProjectStatus,
  QueryOutcome,
  RecentItem,
  Risk,
  RowPage,
  SchemaCheck,
  SchemaFile,
  Settings,
  TableDetail,
  TableSummary,
} from "./types";

/** A command failure, with the structured fields the Rust side sent. */
export class KairoError extends Error {
  readonly kind: ErrorKind;
  readonly detail?: string;
  readonly hint?: string;

  constructor(data: KairoErrorData) {
    super(data.message);
    this.name = "KairoError";
    this.kind = data.kind;
    this.detail = data.detail;
    this.hint = data.hint;
  }
}

function isErrorData(value: unknown): value is KairoErrorData {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as KairoErrorData).kind === "string" &&
    typeof (value as KairoErrorData).message === "string"
  );
}

/** Turns anything a command can reject with into a KairoError. */
export function toKairoError(raw: unknown): KairoError {
  if (raw instanceof KairoError) return raw;
  if (isErrorData(raw)) return new KairoError(raw);
  // A panic or a missing command reaches us as a bare string.
  const detail = raw instanceof Error ? raw.message : String(raw);
  return new KairoError({
    kind: "internal",
    message: "Something went wrong inside Kairo.",
    detail,
  });
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    throw toKairoError(raw);
  }
}

/** Every command the Rust side exposes. Argument keys are camelCase here and snake_case there. */
export const api = {
  appInfo: () => call<AppInfo>("app_info"),

  getSettings: () => call<Settings>("get_settings"),
  setSettings: (settings: Settings) => call<Settings>("set_settings", { settings }),

  listRecents: () => call<RecentItem[]>("list_recents"),
  removeRecent: (key: string) => call<RecentItem[]>("remove_recent", { key }),
  clearRecents: () => call<void>("clear_recents"),

  testConnection: (request: ConnectRequest) =>
    call<ConnectionCheck>("test_connection", { request }),
  connect: (request: ConnectRequest) => call<ConnectionInfo>("connect", { request }),
  connectProject: (dir: string) => call<ConnectionInfo>("connect_project", { dir }),
  disconnect: (connectionId: string) => call<void>("disconnect", { connectionId }),
  listConnections: () => call<ConnectionInfo[]>("list_connections"),

  listTables: (connectionId: string) => call<TableSummary[]>("list_tables", { connectionId }),
  describeTable: (connectionId: string, table: string) =>
    call<TableDetail>("describe_table", { connectionId, table }),
  fetchRows: (connectionId: string, table: string, request: PageRequest) =>
    call<RowPage>("fetch_rows", { connectionId, table, request }),

  analyzeQuery: (sql: string) => call<PreparedQuery>("analyze_query", { sql }),
  runQuery: (connectionId: string, sql: string, acknowledge: Risk | null) =>
    call<QueryOutcome>("run_query", { connectionId, sql, acknowledge }),
  listHistory: (workspaceKey: string) => call<HistoryEntry[]>("list_history", { workspaceKey }),
  clearHistory: (workspaceKey: string) => call<void>("clear_history", { workspaceKey }),

  validateSchema: (text: string, connectionId: string | null) =>
    call<SchemaCheck>("validate_schema", { text, connectionId }),
  readSchemaFile: (path: string) => call<SchemaFile>("read_schema_file", { path }),
  writeSchemaFile: (path: string, content: string) =>
    call<SchemaFile>("write_schema_file", { path, content }),
  planSchema: (connectionId: string, text: string) =>
    call<ApplyPlan>("plan_schema", { connectionId, text }),
  applySchema: (connectionId: string, text: string, confirmed: boolean) =>
    call<ApplyReport>("apply_schema", { connectionId, text, confirmed }),
  exportSchema: (connectionId: string) => call<ExportedSchema>("export_schema", { connectionId }),

  projectStatus: (dir: string) => call<ProjectStatus>("project_status", { dir }),
  initProject: (dir: string) => call<ProjectStatus>("init_project", { dir }),
};

export type Api = typeof api;
