// The wire contract with the Rust side. Each type mirrors a struct in
// kairo-core (db/model.rs, services/*) or the Tauri adapter (commands.rs,
// store.rs), serialised with camelCase field names.

export type Engine = "sqlite" | "postgres";

export type ErrorKind =
  | "invalid_input"
  | "not_found"
  | "invalid_database"
  | "invalid_url"
  | "auth_failed"
  | "network"
  | "tls"
  | "timeout"
  | "busy"
  | "permission_denied"
  | "syntax"
  | "constraint"
  | "schema"
  | "unsupported"
  | "confirmation_required"
  | "not_connected"
  | "io"
  | "database"
  | "internal";

/** What a failed command rejects with. */
export interface KairoErrorData {
  kind: ErrorKind;
  message: string;
  detail?: string;
  hint?: string;
}

export interface AppInfo {
  version: string;
  coreVersion: string;
  platform: string;
  arch: string;
  storePath: string | null;
}

export type Theme = "dark" | "light" | "system";

export interface Settings {
  theme: Theme;
  pageSize: number;
  maxRows: number;
  queryTimeoutSecs: number;
  confirmWrites: boolean;
}

export type RecentKind = "sqlite" | "postgres" | "project";

export interface RecentItem {
  key: string;
  kind: RecentKind;
  name: string;
  location: string;
  /** A file path, a project folder, or a PostgreSQL URL without a password. */
  target: string;
  lastOpened: number;
}

export type ConnectRequest =
  | { kind: "sqlite"; path: string; create?: boolean }
  | { kind: "postgres"; url: string; password?: string | null };

/** An open connection. It never carries a password. */
export interface ConnectionInfo {
  id: string;
  engine: Engine;
  name: string;
  /** The file path, or the URL with its password masked. */
  location: string;
  host: string | null;
  database: string | null;
  username: string | null;
  serverVersion: string;
  readOnly: boolean;
  workspaceKey: string;
  sizeBytes: number | null;
}

export interface ConnectionCheck {
  engine: Engine;
  serverVersion: string;
  location: string;
  tableCount: number;
  latencyMs: number;
}

export type TableKind = "table" | "view";

export interface TableSummary {
  name: string;
  kind: TableKind;
  columnCount: number;
  rowCount: number | null;
  rowCountEstimated: boolean;
}

export interface ColumnInfo {
  name: string;
  dataType: string;
  kairoType: string;
  nullable: boolean;
  defaultValue: string | null;
  /** 1-based position in the primary key, or 0. */
  primaryKeyPosition: number;
}

export interface IndexInfo {
  name: string;
  unique: boolean;
  primary: boolean;
  columns: string[];
  origin: string;
  definition: string | null;
}

export interface ForeignKeyInfo {
  name: string | null;
  columns: string[];
  referencesTable: string;
  referencesColumns: string[];
  onUpdate: string | null;
  onDelete: string | null;
}

export interface TableDetail {
  name: string;
  kind: TableKind;
  columns: ColumnInfo[];
  indexes: IndexInfo[];
  foreignKeys: ForeignKeyInfo[];
  createSql: string;
}

export type ValueKind = "null" | "bool" | "int" | "float" | "text" | "blob";

/** One cell. Numbers arrive as text so 64-bit integers keep every digit. */
export interface Cell {
  kind: ValueKind;
  /** Omitted for null and for empty text. */
  text?: string;
  /** The text was clipped by the core. */
  truncated?: boolean;
}

export interface ResultColumn {
  name: string;
  dataType: string;
}

export interface SortSpec {
  column: string;
  descending: boolean;
}

export interface PageRequest {
  limit: number;
  offset: number;
  filter?: string | null;
  sort?: SortSpec | null;
}

export interface RowPage {
  columns: ResultColumn[];
  rows: Cell[][];
  totalRows: number;
  offset: number;
  limit: number;
  /** The statement that produced the page. */
  sql: string;
}

export type Risk = "read" | "write" | "destructive";

export interface StatementInfo {
  keyword: string;
  risk: Risk;
  reason: string | null;
  preview: string;
  /** False when Kairo does not know the statement; often a typo. */
  recognized: boolean;
}

export interface SqlAnalysis {
  statements: StatementInfo[];
  risk: Risk;
  reasons: string[];
  returnsRows: boolean;
  unbalancedTransaction: boolean;
}

export interface PreparedQuery {
  sql: string;
  translated: boolean;
  analysis: SqlAnalysis;
}

export interface QueryOutcome {
  columns: ResultColumn[];
  rows: Cell[][];
  rowCount: number;
  rowsAffected: number | null;
  truncated: boolean;
  elapsedMs: number;
  statementCount: number;
  executedSql: string;
  translated: boolean;
  risk: Risk;
}

export interface HistoryEntry {
  sql: string;
  at: number;
  ok: boolean;
  risk: Risk;
  elapsedMs: number | null;
  rowCount: number | null;
  rowsAffected: number | null;
  error: string | null;
}

export type Severity = "error" | "warning";

/** 1-based positions; the end is exclusive. */
export interface Diagnostic {
  severity: Severity;
  message: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export type DefaultValue =
  | { kind: "bool"; value: boolean }
  | { kind: "int" | "float" | "text"; value: string };

export interface SchemaField {
  name: string;
  quoted: boolean;
  typeName: string;
  defaultValue: DefaultValue | null;
  required: boolean;
  primary: boolean;
  unique: boolean;
  comment: string | null;
  line: number;
}

export interface SchemaTable {
  name: string;
  quoted: boolean;
  fields: SchemaField[];
  line: number;
}

export interface Schema {
  tables: SchemaTable[];
}

export interface ValidationReport {
  valid: boolean;
  schema: Schema | null;
  diagnostics: Diagnostic[];
}

export interface FieldPreview {
  name: string;
  typeName: string;
  sqlType: string;
  knownType: boolean;
  defaultValue: string | null;
  required: boolean;
  primary: boolean;
  unique: boolean;
  line: number;
}

export interface TablePreview {
  name: string;
  fields: FieldPreview[];
  sql: string;
  line: number;
}

export interface SchemaPreview {
  dialect: Engine;
  tables: TablePreview[];
  sql: string;
}

export interface SchemaCheck {
  report: ValidationReport;
  preview: SchemaPreview | null;
}

export interface SchemaFile {
  path: string;
  name: string;
  content: string;
}

export type PlanAction = "create" | "exists";

export interface PlanItem {
  table: string;
  action: PlanAction;
  sql: string;
  differences: string[];
}

export interface ApplyPlan {
  engine: Engine;
  targetName: string;
  targetLocation: string;
  items: PlanItem[];
  creates: number;
  existing: number;
  sql: string;
}

export interface ApplyReport {
  engine: Engine;
  targetName: string;
  targetLocation: string;
  items: PlanItem[];
  created: number;
  unchanged: number;
  sql: string;
  elapsedMs: number;
}

export interface ExportedSchema {
  text: string;
  tableCount: number;
  notes: string[];
}

export interface ProjectSchemaFile {
  name: string;
  path: string;
}

export interface ProjectStatus {
  root: string;
  initialized: boolean;
  adapter: string | null;
  database: string | null;
  schemaFiles: ProjectSchemaFile[];
  databaseExists: boolean | null;
  problem: string | null;
}
