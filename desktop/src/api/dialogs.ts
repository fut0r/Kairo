import { open, save } from "@tauri-apps/plugin-dialog";

// Native file dialogs. Each resolves to a path, or null when cancelled.

const DATABASE_FILTERS = [
  { name: "SQLite database", extensions: ["db", "sqlite", "sqlite3", "db3"] },
  { name: "All files", extensions: ["*"] },
];
const SCHEMA_FILTERS = [{ name: "Kairo schema", extensions: ["kairo"] }];

export async function pickDatabaseFile(): Promise<string | null> {
  const path = await open({
    title: "Open database",
    multiple: false,
    directory: false,
    filters: DATABASE_FILTERS,
  });
  return typeof path === "string" ? path : null;
}

export async function pickNewDatabaseFile(): Promise<string | null> {
  return save({
    title: "Create database",
    defaultPath: "kairo.db",
    filters: [DATABASE_FILTERS[0]!],
  });
}

export async function pickSchemaFile(): Promise<string | null> {
  const path = await open({
    title: "Open schema",
    multiple: false,
    directory: false,
    filters: SCHEMA_FILTERS,
  });
  return typeof path === "string" ? path : null;
}

export async function pickSchemaSavePath(suggested: string): Promise<string | null> {
  const path = await save({
    title: "Save schema",
    defaultPath: suggested,
    filters: SCHEMA_FILTERS,
  });
  if (!path) return null;
  // The save dialog does not always add the extension.
  return /\.kairo$/i.test(path) ? path : `${path}.kairo`;
}

export async function pickProjectFolder(): Promise<string | null> {
  const path = await open({ title: "Open project folder", multiple: false, directory: true });
  return typeof path === "string" ? path : null;
}
