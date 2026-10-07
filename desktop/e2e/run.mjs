// Drives the real KairoDB desktop app end to end and records what happened.
//
//   node run.mjs <project-dir> <screenshot-dir> <config-dir>
//
// <project-dir> must be a Kairo project (kairo init) whose database exists.

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { connect, sleep, CTRL } from "./cdp.mjs";
import { REPORT_QUERY, SCHEMA, SEED } from "./demo.mjs";

const [projectDir, shotDir, configDir] = process.argv.slice(2);
const dbPath = path.join(projectDir, "data", "shop.db");
const schemaPath = path.join(projectDir, "schema", "shop.kairo");

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok: Boolean(ok), detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${ok || !detail ? "" : `\n        ${detail}`}`);
}

const app = await connect();
const shot = (name) => app.screenshot(path.join(shotDir, `${name}.png`));
const body = () => app.text("body");
const has = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;
const visibleDialog = () => app.evaluate(`document.querySelector('[role="dialog"]')?.innerText ?? ""`);
const tableNames = async (id) => ((await app.invoke("list_tables", { connectionId: id })).ok ?? []).map((t) => t.name);

/** Clicks the visible control whose label is exactly `label`. */
async function press(label, selector = "button") {
  const ok = await app.evaluate(`(() => {
    const wanted = ${JSON.stringify(label)};
    const scope = document.querySelector('[role="dialog"]') ?? document;
    const nodes = [...scope.querySelectorAll(${JSON.stringify(selector)})].filter((n) => n.getClientRects().length > 0);
    const label = (n) => (n.getAttribute("aria-label") ?? n.innerText ?? "").trim();
    const node = nodes.find((n) => label(n) === wanted) ?? nodes.find((n) => label(n).startsWith(wanted));
    if (!node || node.disabled) return false;
    node.click();
    return true;
  })()`);
  if (!ok) throw new Error(`no enabled control labelled "${label}"`);
  await sleep(150);
}
const goTo = (view) => press(view, ".sidebar .nav-item");

try {
  // ───────────── A. The app starts ─────────────
  await app.waitFor(has("Open a database to begin."), "welcome screen");
  const info = (await app.invoke("app_info")).ok;
  check("app starts and reports version 1.0.0", info?.version === "1.0.0" && info?.coreVersion === "1.0.0", JSON.stringify(info));
  check("store lives in the isolated config dir", info?.storePath?.startsWith(configDir), info?.storePath);
  check("sidebar shows brand, version and both primary actions",
    (await app.text(".sidebar")).includes("KairoDB") && (await app.text(".sidebar")).includes("v1.0.0")
      && (await app.text(".sidebar")).includes("Open Database") && (await app.text(".sidebar")).includes("New Connection"));
  check("header says no database is open", (await app.text(".page-header")).includes("No database open"));
  await shot("01-welcome");

  // ───────────── B. Connection dialog: failures, then success ─────────────
  await press("New Connection");
  await app.waitFor(`!!document.querySelector('[role="dialog"]')`, "connection dialog");
  await press("SQLite file", '[role="tab"]');

  await app.type("#sqlite-path", path.join(projectDir, "kairo.config"));
  await press("Test connection");
  await app.waitFor(`!!document.querySelector('[role="dialog"] [role="alert"]')`, "error for a non-database");
  check("a file that is not a database is refused clearly", (await visibleDialog()).includes("Not a database"), await visibleDialog());

  await app.type("#sqlite-path", path.join(projectDir, "data", "missing.db"));
  await press("Test connection");
  await app.waitFor(`(document.querySelector('[role="dialog"] [role="alert"]')?.innerText ?? "").includes("Not found")`, "error for a missing file");
  check("a missing file is reported as not found", true);

  await app.type("#sqlite-path", dbPath);
  await press("Test connection");
  await app.waitFor(has("Connection works"), "successful test");
  check("testing a real SQLite file succeeds and shows its version", /SQLite 3\.\d+/.test(await visibleDialog()), await visibleDialog());
  check("testing does not open or remember anything", (await app.invoke("list_connections")).ok.length === 0 && (await app.invoke("list_recents")).ok.length === 0);

  await press("Open database");
  await app.waitFor(`!document.querySelector('[role="dialog"]')`, "dialog to close");
  await app.waitFor(`document.querySelector(".page-header").innerText.includes("shop.db")`, "header to show the database");
  const connection = (await app.invoke("list_connections")).ok[0];
  let id = connection.id;
  check("a real SQLite database opens through the GUI", connection.engine === "sqlite" && connection.name === "shop.db", JSON.stringify(connection));
  check("opening moves to the Explorer, which explains an empty database",
    (await app.text(".page-header h1")) === "Explorer" && (await body()).includes("This database has no tables"));

  // ───────────── C. Schema: open a real file, validate, preview, apply ─────────────
  await writeFile(schemaPath, SCHEMA, "utf8");
  // The native folder picker cannot be scripted. Registering the project puts
  // it under Recent, and reopening it from there goes through the same code.
  await app.invoke("project_status", { dir: projectDir });
  await app.evaluate("location.reload()");
  await app.waitFor(`document.querySelector(".page-header")?.innerText.includes("shop.db")`, "connection to survive a reload");
  check("an open connection survives a webview reload", true);

  await press(path.basename(projectDir), ".sidebar .nav-item");
  await app.waitFor(has("shop.kairo"), "project card");
  // Opening the project reconnects to its database, which is a new session.
  const before = id;
  for (let i = 0; i < 100 && id === before; i += 1) {
    await sleep(100);
    id = (await app.invoke("list_connections")).ok[0]?.id ?? before;
  }
  check("opening a project connects to its configured database", id !== before);
  check("a project folder shows its config and schema files",
    (await body()).includes("adapter") || (await body()).includes("Adapter"));
  await press("shop.kairo", ".list-row");
  await app.waitFor(`document.querySelector(".page-header h1").innerText === "Schema"`, "schema view");
  await app.waitFor(`(document.querySelector(".view:not([hidden]) .editor-input")?.value ?? "").includes("table customers")`, "the file's text in the editor");
  check("a real .kairo file opens in the editor", true);

  await app.waitFor(has("valid · 3 tables"), "validation result");
  check("the real parser validates it: 3 tables", true);
  const preview = await app.text(".view:not([hidden])");
  check("the preview shows parsed fields with their database types",
    preview.includes("customers") && preview.includes("→ INTEGER") && preview.includes("→ REAL") && preview.includes("→ BLOB") && preview.includes("primary") && preview.includes("required"));
  await shot("05-schema");

  await press("SQLite SQL", '[role="tab"]');
  await app.waitFor(has("CREATE TABLE IF NOT EXISTS customers ("), "generated SQL");
  check("the SQL tab shows exactly what will run", (await body()).includes("email TEXT NOT NULL UNIQUE") && (await body()).includes("status TEXT NOT NULL DEFAULT 'pending'"));
  await press("Preview", '[role="tab"]');

  // Break it: inline feedback, and applying is blocked.
  const broken = SCHEMA.replace("name: string [required]\n  email", "name string [required]\n  email");
  await app.type(".view:not([hidden]) .editor-input", broken);
  await app.waitFor(has("Expected `:` after `name`"), "inline diagnostic");
  const problems = await app.text(".view:not([hidden])");
  check("a malformed schema is reported inline with line and column", problems.includes("5:3") && problems.includes("1 error"), problems.slice(0, 400));
  check("the editor marks the failing line", await app.evaluate(`document.querySelector(".view:not([hidden]) .editor-gutter .has-error")?.innerText === "5"`));
  check("applying is blocked while there are errors",
    await app.evaluate(`[...document.querySelectorAll(".view:not([hidden]) .toolbar .btn-primary")].some((b) => b.disabled && b.innerText.includes("Fix the errors first"))`));
  await shot("06-schema-error");

  await app.type(".view:not([hidden]) .editor-input", SCHEMA);
  await app.waitFor(has("valid · 3 tables"), "schema valid again");

  await press("Apply schema…");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("will be created")`, "apply plan");
  const plan = await visibleDialog();
  check("the apply dialog names the target database and what will happen",
    plan.includes("shop.db") && plan.includes(dbPath) && plan.includes("3 tables will be created") && plan.includes("CREATE TABLE IF NOT EXISTS orders"), plan.slice(0, 500));
  check("nothing is created until the dialog is confirmed", (await tableNames(id)).length === 0);
  check("focus starts on Cancel in the apply dialog", await app.evaluate(`document.activeElement?.innerText === "Cancel"`));
  await shot("07-apply-plan");

  await press("Create 3 tables in shop.db");
  await app.waitFor(has("Applied to shop.db"), "apply result");
  const created = await tableNames(id);
  check("the schema is applied to the selected database", created.join() === "customers,orders,products", created.join());

  await press("Apply schema…");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("already exists")`, "second plan");
  const second = await visibleDialog();
  check("applying again says the tables exist and offers nothing to do",
    second.includes("Every table in this schema already exists") && second.includes("Nothing to create") && second.includes("matches"), second.slice(0, 400));
  await press("Cancel");

  const unconfirmed = await app.invoke("apply_schema", { connectionId: id, text: SCHEMA, confirmed: false });
  check("the backend refuses apply_schema without confirmation", unconfirmed.err?.kind === "confirmation_required", JSON.stringify(unconfirmed));

  // ───────────── D. Query: confirmation, results, errors, history ─────────────
  await goTo("Query");
  await app.waitFor(`!!document.querySelector(".view:not([hidden]) .editor-input")`, "query editor");
  check("an empty query workspace explains what to do", (await body()).includes("Results appear here"));

  await app.type(".view:not([hidden]) .editor-input", SEED);
  await press("Run");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("destructive")`, "confirmation for the seed script");
  const confirmSeed = await visibleDialog();
  check("a script that changes rows asks first and says why",
    confirmSeed.includes("Run a destructive statement?") && confirmSeed.includes("UPDATE changes existing rows.") && confirmSeed.includes("shop.db"), confirmSeed.slice(0, 500));
  check("nothing ran while the dialog was open", (await tableNames(id)).join() === "customers,orders,products");
  await shot("04-confirm");
  await press("Run anyway");
  await app.waitFor(has("Statement finished"), "seed to finish", 30000);
  const seeded = await tableNames(id);
  check("after confirming, the whole script runs", seeded.join() === "customer_spend,customers,order_items,orders,products", seeded.join());

  await app.type(".view:not([hidden]) .editor-input", REPORT_QUERY);
  await app.key("Enter", CTRL);
  await app.waitFor(`!!document.querySelector('.view:not([hidden]) [role="grid"][aria-label="Query results"]')`, "result grid");
  const result = await app.text(".view:not([hidden])");
  check("a SELECT runs from Ctrl+Enter without a prompt and returns structured rows",
    result.includes("25 rows") && result.includes("4 columns") && /\d ms|<1 ms/.test(result) && result.includes("spent"), result.slice(0, 300));
  await shot("03-query");

  await app.type(".view:not([hidden]) .editor-input", "from customers where city = 'Cairo'");
  await press("Run");
  await app.waitFor(has("Ran as:"), "translated query");
  check("the short form is expanded and the exact SQL is shown", (await body()).includes("SELECT * from customers where city = 'Cairo'") && (await body()).includes("8 rows"));

  await app.type(".view:not([hidden]) .editor-input", "SELECT * FROM ghosts");
  await press("Run");
  await app.waitFor(`!!document.querySelector('.view:not([hidden]) [role="alert"]')`, "query error");
  const queryError = await app.text('.view:not([hidden]) [role="alert"]');
  check("a failing query shows a structured error", queryError.includes("SQL error") && queryError.includes("no such table: ghosts"), queryError);

  await app.type(".view:not([hidden]) .editor-input", "DROP TABLE order_items");
  await press("Run");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("DROP permanently removes the table")`, "drop confirmation");
  await press("Cancel");
  await sleep(300);
  check("cancelling a DROP leaves the table in place", (await tableNames(id)).includes("order_items"));

  const noAck = await app.invoke("run_query", { connectionId: id, sql: "DROP TABLE order_items", acknowledge: null });
  const weakAck = await app.invoke("run_query", { connectionId: id, sql: "DROP TABLE order_items", acknowledge: "write" });
  check("the backend refuses destructive SQL without a matching acknowledgement",
    noAck.err?.kind === "confirmation_required" && weakAck.err?.kind === "confirmation_required" && (await tableNames(id)).includes("order_items"),
    JSON.stringify([noAck, weakAck]));

  const big = await app.invoke("run_query", { connectionId: id, sql: "SELECT 9007199254740993 AS n, x'00ff' AS b, NULL AS z, 1.5 AS f", acknowledge: null });
  const cells = big.ok?.rows?.[0] ?? [];
  check("values arrive typed, and 64-bit integers keep every digit",
    cells[0]?.text === "9007199254740993" && cells[0]?.kind === "int" && cells[1]?.kind === "blob" && cells[2]?.kind === "null" && cells[3]?.text === "1.5", JSON.stringify(cells));

  const history = await app.text('.view:not([hidden]) [aria-label="Query history"]');
  check("history lists what was run in this workspace, including the failure", history.includes("from customers where city") && history.includes("failed") && history.includes("SELECT * FROM ghosts"), history.slice(0, 300));

  // ───────────── E. Explorer ─────────────
  await goTo("Explorer");
  await app.waitFor(`document.querySelectorAll(".view:not([hidden]) .tree-row").length === 5`, "five tables listed");
  const tree = await app.text('.view:not([hidden]) [aria-label="Tables"]');
  check("tables and the view are listed with row counts", tree.includes("customers") && tree.includes("64") && tree.includes("order_items") && tree.includes("420") && tree.includes("customer_spend"), tree);

  await press("customers", ".view:not([hidden]) .tree-main");
  await app.waitFor(has("1–50 of 64"), "first page of customers");
  check("rows are paginated: 1–50 of 64", true);
  const firstPage = await app.text('.view:not([hidden]) [role="grid"]');
  check("real rows render, including right-to-left text and NULL", firstPage.includes("ليلى حداد") && firstPage.includes("NULL") && firstPage.includes("@example.com"), firstPage.slice(0, 200));
  check("the statement behind the page is shown", (await body()).includes('SELECT * FROM "customers" LIMIT 50 OFFSET 0'));
  await shot("02-explorer");

  await press("Next page");
  await app.waitFor(has("51–64 of 64"), "second page");
  check("next page shows 51–64 of 64 and stops there",
    await app.evaluate(`document.querySelector('.view:not([hidden]) [aria-label="Next page"]').disabled`));
  await press("First page");
  await app.waitFor(has("1–50 of 64"), "back to the first page");

  await app.evaluate(`(() => { const s = document.querySelector(".view:not([hidden]) #page-size"); s.value = "25";
    s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  await app.waitFor(has("1–25 of 64"), "page size 25");
  check("the page size control works", true);

  await app.type(".view:not([hidden]) #row-filter", "cairo");
  await app.waitFor(has("1–8 of 8 matching"), "filtered rows");
  check("filtering searches every column, case-insensitively", (await body()).includes("LIKE ?1"));
  await app.type(".view:not([hidden]) #row-filter", "' OR 1=1 --");
  await app.waitFor(has("No rows match"), "injection attempt finds nothing");
  check("a filter is data, not SQL", (await tableNames(id)).length === 5);
  await press("Clear filter");
  await app.waitFor(has("of 64"), "filter cleared");

  await press("joined_at", ".view:not([hidden]) .col-head");
  await app.waitFor(has('ORDER BY "joined_at" ASC'), "sorted ascending");
  await press("joined_at", ".view:not([hidden]) .col-head");
  await app.waitFor(has('ORDER BY "joined_at" DESC'), "sorted descending");
  check("clicking a column header sorts, then reverses", true);

  await app.evaluate(`document.querySelector('.view:not([hidden]) [role="grid"]').focus()`);
  await app.key("ArrowDown");
  await app.key("ArrowRight");
  await app.key("ArrowDown");
  check("the grid is navigable by keyboard and shows the active cell",
    await app.evaluate(`!!document.querySelector(".view:not([hidden]) .cell-active") && !!document.querySelector(".view:not([hidden]) .cell-detail")`));

  await press("Show columns of orders");
  await app.waitFor(`document.querySelectorAll(".view:not([hidden]) .tree-child").length >= 5`, "expanded columns");
  check("a table expands to show its columns and types", (await app.text(".view:not([hidden]) .tree-children")).includes("customer_id"));

  await press("order_items", ".view:not([hidden]) .tree-main");
  await press("Structure", '.view:not([hidden]) [role="tab"]');
  await app.waitFor(has("NULLABLE"), "structure tab");
  const structure = await app.text('.view:not([hidden]) [aria-label="Table detail"]');
  check("structure shows types, nullability, defaults and keys",
    structure.includes("primary key") && structure.includes("not null") && structure.includes("→ orders.id") && structure.includes("real"), structure.slice(0, 500));
  await shot("08-structure");

  await press("Indexes", '.view:not([hidden]) [role="tab"]');
  await app.waitFor(has("order_items_order_idx"), "indexes tab");
  const indexes = await app.text('.view:not([hidden]) [aria-label="Table detail"]');
  check("indexes and foreign keys are listed", indexes.includes("order_id") && indexes.includes("CASCADE") && indexes.includes("products"));

  await press("SQL", '.view:not([hidden]) [role="tab"]');
  await app.waitFor(has("CREATE TABLE order_items"), "sql tab");
  check("the SQL tab shows the table definition and its indexes", (await body()).includes("CREATE INDEX order_items_order_idx"));

  await press("products", ".view:not([hidden]) .tree-main");
  await press("Data", '.view:not([hidden]) [role="tab"]');
  await app.waitFor(has("of 24"), "products rows");
  check("blobs are summarised, not dumped", (await app.text('.view:not([hidden]) [role="grid"]')).includes("<blob 8 bytes>"));

  // ───────────── F. Overview and export ─────────────
  await goTo("Overview");
  await app.waitFor(has("TABLES"), "overview stats");
  const overview = await app.text(".view:not([hidden])");
  check("overview summarises the database and the project", overview.includes("688") && overview.includes("SCHEMAS (1)") && overview.includes("exists"), overview.slice(0, 400));
  await shot("09-overview");

  await goTo("Export");
  await app.waitFor(has("table customers {"), "exported schema");
  const exported = (await app.invoke("export_schema", { connectionId: id })).ok;
  check("export produces .kairo text for every table", exported.tableCount === 4 && exported.text.includes("  email: string [required, unique]") && exported.text.includes("  joined_at: timestamp"), exported.text.slice(0, 300));
  const reparsed = (await app.invoke("validate_schema", { text: exported.text, connectionId: id })).ok;
  check("the exported text is valid for the same parser", reparsed.report.valid === true, JSON.stringify(reparsed.report.diagnostics));
  await shot("10-export");

  const savedPath = path.join(projectDir, "schema", "exported.kairo");
  const saved = await app.invoke("write_schema_file", { path: savedPath, content: exported.text });
  check("the export can be saved as a .kairo file", saved.ok?.name === "exported.kairo" && (await readFile(savedPath, "utf8")) === exported.text);
  const refused = await app.invoke("write_schema_file", { path: path.join(projectDir, "notes.txt"), content: "x" });
  const refusedRead = await app.invoke("read_schema_file", { path: path.join(projectDir, "kairo.config") });
  check("the file commands only touch .kairo files", refused.err?.kind === "invalid_input" && refusedRead.err?.kind === "invalid_input");

  await press("Open in Schema workspace");
  await app.waitFor(`document.querySelector(".page-header h1").innerText === "Schema"`, "schema view from export");
  await app.waitFor(has("valid · 4 tables"), "exported schema validates in the editor");
  check("an export opens in the Schema workspace as an unsaved file", (await body()).includes("shop.kairo") && (await body()).includes("●"));

  // ───────────── G. Credentials ─────────────
  await press("New Connection");
  await app.waitFor(`!!document.querySelector("#pg-url")`, "postgres form");
  await app.type("#pg-url", "postgres://kairo:hunter2@127.0.0.1:1/app");
  await app.evaluate(`document.querySelector("#pg-password").focus()`);
  await sleep(150);
  check("a password typed into the URL is masked once the field loses focus",
    (await app.evaluate(`document.querySelector("#pg-url").value`)) === "postgres://kairo:••••@127.0.0.1:1/app");
  await press("Test connection");
  await app.waitFor(`!!document.querySelector('[role="dialog"] [role="alert"]')`, "postgres failure", 20000);
  const pgError = await visibleDialog();
  check("an unreachable PostgreSQL server gives a specific error", pgError.includes("Can't reach the server") && pgError.includes("refused"), pgError.slice(0, 300));
  check("no password appears anywhere on screen", !(await body()).includes("hunter2"));
  await shot("11-postgres");
  await press("Cancel");

  for (const [label, url, kind] of [
    ["malformed URL", "postgres://kairo:hunter2@host:notaport/app", "invalid_url"],
    ["refused connection", "postgres://kairo:hunter2@127.0.0.1:1/app", "network"],
  ]) {
    const failed = await app.invoke("connect", { request: { kind: "postgres", url } });
    check(`${label}: structured ${kind} error with no password in it`, failed.err?.kind === kind && !JSON.stringify(failed).includes("hunter2"), JSON.stringify(failed));
  }

  // ───────────── H. Settings and persistence ─────────────
  await goTo("Settings");
  await press("Light", '[role="radio"]');
  await app.waitFor(`document.documentElement.dataset.theme === "light"`, "light theme");
  await goTo("Explorer");
  await sleep(300);
  await shot("12-light");
  await goTo("Settings");
  await press("Dark", '[role="radio"]');
  await app.waitFor(`document.documentElement.dataset.theme === "dark"`, "dark theme");
  check("the theme switches between dark and light", true);
  await shot("13-settings");

  await app.key("2", CTRL);
  await app.waitFor(`document.querySelector(".page-header h1").innerText === "Explorer"`, "Ctrl+2");
  check("Ctrl+number switches views", true);

  const store = await readFile(path.join(configDir, "kairo.json"), "utf8");
  const parsed = JSON.parse(store);
  check("recents and history are saved to the store", parsed.recents.some((r) => r.kind === "sqlite" && r.name === "shop.db") && parsed.recents.some((r) => r.kind === "project") && Object.values(parsed.history)[0].length >= 4);
  check("the store on disk contains no password", !store.includes("hunter2"), store.slice(0, 200));

  await press("Close shop.db");
  await app.waitFor(`document.querySelector(".page-header").innerText.includes("No database open")`, "disconnect");
  check("closing a connection returns to the no-database state", (await app.invoke("list_connections")).ok.length === 0);
  const gone = await app.invoke("list_tables", { connectionId: id });
  check("commands on a closed connection fail cleanly", gone.err?.kind === "not_connected");
} catch (error) {
  check(`scenario aborted: ${error.message}`, false, (await body().catch(() => "")).slice(0, 600));
  await shot("zz-failure").catch(() => {});
}

app.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length} passed, ${failed.length} failed`);
process.exit(failed.length === 0 ? 0 : 1);
