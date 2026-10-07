// Drives the desktop app against a PostgreSQL server, through the same
// dialog and screens as SQLite.
//
//   node postgres.mjs <postgres-url-with-password> <screenshot-dir> <config-dir>
//
// It creates and drops three tables (customers, products, orders) in the
// connection's schema. Point it at a scratch database.

import { readFile } from "node:fs/promises";
import path from "node:path";
import { connect, sleep } from "./cdp.mjs";
import { SCHEMA } from "./demo.mjs";

const [url, shotDir, configDir] = process.argv.slice(2);
const password = /:\/\/[^:/@]+:([^@]+)@/.exec(url)?.[1] ?? "";
const withoutPassword = url.replace(`:${password}@`, "@");

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok: Boolean(ok) });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${ok || !detail ? "" : `\n        ${detail}`}`);
}

const app = await connect();
const body = () => app.text("body");
const has = (text) => `document.body.innerText.includes(${JSON.stringify(text)})`;
const dialog = () => app.evaluate(`document.querySelector('[role="dialog"]')?.innerText ?? ""`);

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

try {
  await app.waitFor(has("Open a database to begin."), "welcome screen");

  // ── Connect through the dialog: URL without the password, password apart ──
  await press("New Connection");
  await app.waitFor(`!!document.querySelector("#pg-url")`, "postgres form");
  await app.type("#pg-url", withoutPassword);
  await app.type("#pg-password", password);
  await press("Test connection");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("Connection works")`, "successful test", 30000);
  const tested = await dialog();
  check("testing a PostgreSQL connection reports the server version", /PostgreSQL \d+/.test(tested), tested);
  check("the test result shows the URL with the password masked", tested.includes(":••••@") && !tested.includes(password), tested);
  check("testing remembers nothing", (await app.invoke("list_recents")).ok.length === 0);

  await press("Connect");
  await app.waitFor(`!document.querySelector('[role="dialog"]')`, "dialog to close", 30000);
  await app.waitFor(`document.querySelector(".page-header").innerText.includes("PostgreSQL")`, "header");
  const connection = (await app.invoke("list_connections")).ok[0];
  const id = connection.id;
  check("PostgreSQL connects through the GUI", connection.engine === "postgres" && connection.serverVersion.startsWith("PostgreSQL"), JSON.stringify(connection));
  check("nothing the UI is given contains the password", !JSON.stringify(connection).includes(password) && !(await body()).includes(password));
  check("the header shows host and a masked URL", (await app.text(".page-header")).includes(":••••@"));

  // ── Schema: the same workspace, PostgreSQL dialect ──
  await app.invoke("run_query", { connectionId: id, sql: "DROP TABLE IF EXISTS orders; DROP TABLE IF EXISTS products; DROP TABLE IF EXISTS customers;", acknowledge: "destructive" });
  await press("Schema", ".sidebar .nav-item");
  await app.waitFor(`!!document.querySelector(".view:not([hidden]) .editor-input")`, "schema editor");
  await app.type(".view:not([hidden]) .editor-input", SCHEMA);
  await app.waitFor(has("valid · 3 tables"), "validation");
  await press("PostgreSQL", '[role="tab"]');
  await app.waitFor(has("DOUBLE PRECISION"), "postgres dialect in the SQL tab");
  check("the SQL preview uses the PostgreSQL dialect", (await body()).includes("price DOUBLE PRECISION NOT NULL DEFAULT 0.0") && (await body()).includes("photo BYTEA"));

  await press("Apply schema…");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("will be created")`, "apply plan", 30000);
  const plan = await dialog();
  check("the apply dialog names the PostgreSQL target without the password", plan.includes("PostgreSQL") && plan.includes(":••••@") && !plan.includes(password), plan.slice(0, 300));
  await press("Create 3 tables");
  await app.waitFor(has("Applied to"), "apply result", 30000);
  const tables = ((await app.invoke("list_tables", { connectionId: id })).ok ?? []).map((t) => t.name);
  check("the schema is applied to PostgreSQL", ["customers", "orders", "products"].every((t) => tables.includes(t)), tables.join());

  // ── Query and Explorer ──
  const seed = `INSERT INTO customers (id, name, email, city, active, joined_at)
    SELECT i, 'Customer ' || i, 'c' || i || '@example.com', (ARRAY['Cairo','Osaka','Lagos'])[1 + i % 3], i % 4 <> 0,
           timestamp '2025-01-01' + (i || ' days')::interval
      FROM generate_series(1, 75) AS g(i)`;
  await press("Query", ".sidebar .nav-item");
  await app.waitFor(`!!document.querySelector(".view:not([hidden]) .editor-input")`, "query editor");
  await app.type(".view:not([hidden]) .editor-input", seed);
  await press("Run");
  await app.waitFor(`(document.querySelector('[role="dialog"]')?.innerText ?? "").includes("changes data")`, "write confirmation");
  check("a write to PostgreSQL asks first", (await dialog()).includes("INSERT adds rows."));
  await press("Run");
  await app.waitFor(has("75 rows affected"), "insert result", 30000);
  check("the insert reports 75 rows affected", true);

  await app.type(".view:not([hidden]) .editor-input", "SELECT city, count(*) AS people, min(joined_at) AS first_joined, now() AS checked_at, gen_random_uuid() AS token FROM customers GROUP BY city ORDER BY city");
  await press("Run");
  await app.waitFor(`!!document.querySelector('.view:not([hidden]) [role="grid"][aria-label="Query results"]')`, "result grid", 30000);
  const grid = await app.text('.view:not([hidden]) [role="grid"]');
  check("PostgreSQL results render, including timestamp and uuid values", grid.includes("Cairo") && grid.includes("25") && grid.includes("timestamptz") && grid.includes("uuid"), grid.slice(0, 300));
  await app.screenshot(path.join(shotDir, "pg-query.png"));

  await press("Explorer", ".sidebar .nav-item");
  await app.waitFor(`document.querySelectorAll(".view:not([hidden]) .tree-row").length >= 3`, "tables listed");
  await press("customers", ".view:not([hidden]) .tree-main");
  await app.waitFor(has("1–50 of 75"), "first page", 30000);
  const rows = await app.text('.view:not([hidden]) [role="grid"]');
  check("PostgreSQL rows page in key order with typed cells", rows.includes("Customer 1") && rows.includes("Customer 50") && !rows.includes("Customer 51") && rows.includes("true"));
  check("the page statement casts on the server and orders by the key column", (await body()).includes('ORDER BY "customers"."id"'));
  await app.type(".view:not([hidden]) #row-filter", "OSAKA");
  await app.waitFor(has("of 25 matching"), "ILIKE filter", 30000);
  check("filtering is case-insensitive on PostgreSQL", true);
  await press("Structure", '.view:not([hidden]) [role="tab"]');
  await app.waitFor(has("NULLABLE"), "structure tab");
  const structure = await app.text('.view:not([hidden]) [aria-label="Table detail"]');
  check("PostgreSQL structure shows native types and keys", structure.includes("timestamp without time zone") && structure.includes("primary key") && structure.includes("boolean"), structure.slice(0, 400));
  await app.screenshot(path.join(shotDir, "pg-explorer.png"));

  await press("Export", ".sidebar .nav-item");
  await app.waitFor(has("table customers {"), "export", 30000);
  const exported = await app.text(".view:not([hidden])");
  check("PostgreSQL exports to .kairo without the password", exported.includes("email: string [required, unique]") && exported.includes("Source (PostgreSQL)") && !exported.includes(password));

  // ── What is remembered ──
  const store = await readFile(path.join(configDir, "kairo.json"), "utf8");
  const recent = JSON.parse(store).recents.find((r) => r.kind === "postgres");
  check("the saved connection has host, port, database and user, and no password", recent && recent.target.startsWith("postgres://") && !store.includes(password), store.slice(0, 300));

  await app.invoke("run_query", { connectionId: id, sql: "DROP TABLE IF EXISTS orders; DROP TABLE IF EXISTS products; DROP TABLE IF EXISTS customers;", acknowledge: "destructive" });
  await press(`Close ${connection.name}`);
  await app.waitFor(`document.querySelector(".page-header").innerText.includes("No database open")`, "disconnect");

  // Reconnecting from Recent has to ask for the password again.
  await press(recent.name, ".sidebar .nav-item");
  await app.waitFor(`!!document.querySelector("#pg-password")`, "reconnect dialog");
  check("reopening a saved PostgreSQL connection asks for the password",
    (await app.evaluate(`document.querySelector("#pg-url").value`)) === recent.target
      && (await app.evaluate(`document.querySelector("#pg-password").value`)) === ""
      && (await app.evaluate(`document.activeElement?.id`)) === "pg-password");
} catch (error) {
  check(`scenario aborted: ${error.message}`, false, (await body().catch(() => "")).slice(0, 500));
  await app.screenshot(path.join(shotDir, "pg-failure.png")).catch(() => {});
}

app.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length} passed, ${failed.length} failed`);
process.exit(failed.length === 0 ? 0 : 1);
