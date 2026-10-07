// Development fixtures for the test suite only. Nothing under src/ outside
// *.test.* files imports this module, so none of it can reach a screen.

import type {
  AppInfo,
  ConnectionInfo,
  PreparedQuery,
  QueryOutcome,
  Risk,
  Settings,
} from "../api/types";

export const appInfo: AppInfo = {
  version: "1.0.0",
  coreVersion: "1.0.0",
  platform: "windows",
  arch: "x86_64",
  storePath: "C:\\Users\\dev\\AppData\\Roaming\\site.arabdev.kairo\\kairo.json",
};

export const settings: Settings = {
  theme: "dark",
  pageSize: 50,
  maxRows: 1000,
  queryTimeoutSecs: 30,
  confirmWrites: true,
};

export const sqliteConnection: ConnectionInfo = {
  id: "c1",
  engine: "sqlite",
  name: "shop.db",
  location: "C:\\data\\shop.db",
  host: null,
  database: null,
  username: null,
  serverVersion: "SQLite 3.46.0",
  readOnly: false,
  workspaceKey: "sqlite:C:\\data\\shop.db",
  sizeBytes: 8192,
};

export function prepared(sql: string, risk: Risk, reasons: string[] = []): PreparedQuery {
  return {
    sql,
    translated: false,
    analysis: {
      statements: [
        {
          keyword: sql.split(/\s+/)[0]!.toUpperCase(),
          risk,
          reason: reasons[0] ?? null,
          preview: sql,
          recognized: true,
        },
      ],
      risk,
      reasons,
      returnsRows: risk === "read",
      unbalancedTransaction: false,
    },
  };
}

export function outcome(overrides: Partial<QueryOutcome> = {}): QueryOutcome {
  return {
    columns: [
      { name: "id", dataType: "INTEGER" },
      { name: "name", dataType: "TEXT" },
    ],
    rows: [
      [
        { kind: "int", text: "1" },
        { kind: "text", text: "alice" },
      ],
      [{ kind: "int", text: "2" }, { kind: "null" }],
    ],
    rowCount: 2,
    rowsAffected: null,
    truncated: false,
    elapsedMs: 3,
    statementCount: 1,
    executedSql: "SELECT id, name FROM users",
    translated: false,
    risk: "read",
    ...overrides,
  };
}
