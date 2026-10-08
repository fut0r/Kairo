import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api/client";
import type * as ClientModule from "../api/client";
import { ConnectionDialog } from "../components/ConnectionDialog";
import { AppProvider, useApp } from "../state/app";
import { appInfo, outcome, prepared, settings, sqliteConnection } from "../test/fixtures";
import { Explorer } from "./Explorer";
import { QueryView } from "./QueryView";

// The Tauri bridge does not exist under test. Every command is replaced, so a
// test can assert exactly which ones a screen calls and with what.
vi.mock("../api/client", async (original) => {
  const actual = await original<typeof ClientModule>();
  const commands = Object.fromEntries(Object.keys(actual.api).map((name) => [name, vi.fn()]));
  return { ...actual, api: commands };
});
vi.mock("../api/dialogs", () => ({
  pickDatabaseFile: vi.fn(),
  pickNewDatabaseFile: vi.fn(),
  pickSchemaFile: vi.fn(),
  pickSchemaSavePath: vi.fn(),
  pickProjectFolder: vi.fn(),
}));

const mocked = vi.mocked(api);

beforeEach(() => {
  mocked.appInfo.mockResolvedValue(appInfo);
  mocked.getSettings.mockResolvedValue(settings);
  mocked.listRecents.mockResolvedValue([]);
  mocked.listConnections.mockResolvedValue([sqliteConnection]);
  mocked.listHistory.mockResolvedValue([]);
  mocked.runQuery.mockResolvedValue(outcome());
});

async function openQueryView() {
  render(
    <AppProvider>
      <QueryView />
    </AppProvider>,
  );
  return (await screen.findByRole("textbox", { name: "SQL query" })) as HTMLTextAreaElement;
}

function typeAndRun(field: HTMLTextAreaElement, sql: string) {
  fireEvent.change(field, { target: { value: sql } });
  fireEvent.click(screen.getByRole("button", { name: "Run" }));
}

describe("Query workspace", () => {
  it("runs a read straight away and shows its rows", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(prepared("SELECT id, name FROM users", "read"));

    typeAndRun(field, "SELECT id, name FROM users");

    const grid = await screen.findByRole("grid", { name: "Query results" });
    expect(within(grid).getByText("alice")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mocked.runQuery).toHaveBeenCalledWith("c1", "SELECT id, name FROM users", null);
  });

  it("does not run a destructive statement until it is confirmed", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(
      prepared("DROP TABLE users", "destructive", [
        "DROP permanently removes the table and what it holds.",
      ]),
    );
    mocked.runQuery.mockResolvedValue(
      outcome({ columns: [], rows: [], rowCount: 0, rowsAffected: 0, risk: "destructive" }),
    );

    typeAndRun(field, "DROP TABLE users");

    const dialog = await screen.findByRole("dialog", { name: "Run a destructive statement?" });
    expect(within(dialog).getByText("DROP permanently removes the table and what it holds.")).toBeTruthy();
    // The dialog names the database that would be changed.
    expect(within(dialog).getByText("shop.db")).toBeTruthy();
    expect(within(dialog).getByText("C:\\data\\shop.db")).toBeTruthy();
    // Focus starts on the choice that changes nothing.
    expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(mocked.runQuery).not.toHaveBeenCalled();

    fireEvent.click(within(dialog).getByRole("button", { name: "Run anyway" }));

    await waitFor(() =>
      expect(mocked.runQuery).toHaveBeenCalledWith("c1", "DROP TABLE users", "destructive"),
    );
    expect(await screen.findByText("Statement finished")).toBeTruthy();
  });

  it("runs nothing when the confirmation is cancelled", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(prepared("DELETE FROM users", "destructive", ["x"]));

    typeAndRun(field, "DELETE FROM users");
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(mocked.runQuery).not.toHaveBeenCalled();
    // The statement is still in the editor.
    expect(field.value).toBe("DELETE FROM users");
  });

  it("asks before a plain write only while that setting is on", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(
      prepared("INSERT INTO t VALUES (1)", "write", ["INSERT adds rows."]),
    );

    typeAndRun(field, "INSERT INTO t VALUES (1)");
    expect(await screen.findByRole("dialog", { name: "Run a statement that changes data?" })).toBeTruthy();
    expect(mocked.runQuery).not.toHaveBeenCalled();
  });

  it("describes an unrecognised statement as unknown, not as destructive", async () => {
    const field = await openQueryView();
    const typo = prepared("SELEC 1", "destructive", [
      "Kairo does not recognise SELEC, so it asks before running it.",
    ]);
    typo.analysis.statements[0]!.recognized = false;
    mocked.analyzeQuery.mockResolvedValue(typo);

    typeAndRun(field, "SELEC 1");

    const dialog = await screen.findByRole("dialog", {
      name: "Run a statement Kairo does not recognise?",
    });
    expect(within(dialog).getByText(/If it is a typo, cancel and fix it/)).toBeTruthy();
    expect(within(dialog).queryByText(/remove or overwrite data/)).toBeNull();
    // It is still gated: nothing has run.
    expect(mocked.runQuery).not.toHaveBeenCalled();
  });

  it("shows a database error with its detail and keeps the SQL", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(prepared("SELEC 1", "destructive", ["unknown"]));
    mocked.analyzeQuery.mockRejectedValueOnce({
      kind: "syntax",
      message: 'near "SELEC": syntax error',
    });

    typeAndRun(field, "SELEC 1");

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("SQL error")).toBeTruthy();
    expect(within(alert).getByText('near "SELEC": syntax error')).toBeTruthy();
    expect(field.value).toBe("SELEC 1");
    expect(mocked.runQuery).not.toHaveBeenCalled();
  });

  it("says when rows were left out", async () => {
    const field = await openQueryView();
    mocked.analyzeQuery.mockResolvedValue(prepared("SELECT * FROM big", "read"));
    mocked.runQuery.mockResolvedValue(outcome({ truncated: true }));

    typeAndRun(field, "SELECT * FROM big");
    expect(await screen.findByText(/more rows exist/)).toBeTruthy();
  });
});

describe("Explorer", () => {
  beforeEach(() => {
    mocked.listTables.mockResolvedValue([
      { name: "users", kind: "table", columnCount: 2, rowCount: 120, rowCountEstimated: false },
    ]);
    mocked.describeTable.mockResolvedValue({
      name: "users",
      kind: "table",
      columns: [],
      indexes: [],
      foreignKeys: [],
      createSql: "CREATE TABLE users (id INTEGER, name TEXT)",
    });
    mocked.fetchRows.mockImplementation(async (_id, _table, request) => ({
      columns: outcome().columns,
      rows: outcome().rows,
      totalRows: 120,
      offset: request.offset,
      limit: request.limit,
      sql: `SELECT * FROM "users" LIMIT ${request.limit} OFFSET ${request.offset}`,
    }));
  });

  it("keeps a page change made straight after a table opens", async () => {
    render(
      <AppProvider>
        <Explorer />
      </AppProvider>,
    );
    // The paging buttons are disabled until the first page has loaded.
    await screen.findByText(/of 120/);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));

    // Longer than the filter debounce, which used to reset the page.
    await new Promise((resolve) => setTimeout(resolve, 450));

    const offsets = mocked.fetchRows.mock.calls.map((call) => call[2].offset);
    expect(offsets[offsets.length - 1]).toBe(50);
    expect(await screen.findByText(/OFFSET 50/)).toBeTruthy();
  });

  it("filters after typing pauses, from the first page", async () => {
    render(
      <AppProvider>
        <Explorer />
      </AppProvider>,
    );
    await screen.findByText(/of 120/);
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    fireEvent.change(screen.getByLabelText("Filter rows of users"), { target: { value: " ali " } });

    await waitFor(() => {
      const last = mocked.fetchRows.mock.calls[mocked.fetchRows.mock.calls.length - 1]![2];
      expect(last).toMatchObject({ filter: "ali", offset: 0 });
    });
  });
});

describe("Query workspace without a database", () => {
  it("teaches the next step instead of showing an empty editor", async () => {
    mocked.listConnections.mockResolvedValue([]);
    render(
      <AppProvider>
        <QueryView />
      </AppProvider>,
    );
    expect(await screen.findByText("Open a database to run queries.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open Database" })).toBeTruthy();
    expect(screen.queryByRole("textbox")).toBeNull();
  });
});

describe("Connection dialog", () => {
  function Opener() {
    const { openConnectDialog } = useApp();
    return (
      <button type="button" onClick={() => openConnectDialog({ tab: "postgres" })}>
        open
      </button>
    );
  }

  async function openDialog() {
    mocked.listConnections.mockResolvedValue([]);
    render(
      <AppProvider>
        <Opener />
        <ConnectionDialog />
      </AppProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "open" }));
    return (await screen.findByLabelText("Connection URL")) as HTMLInputElement;
  }

  it("hides a typed password as soon as the URL field loses focus", async () => {
    const url = await openDialog();

    fireEvent.focus(url);
    fireEvent.change(url, { target: { value: "postgres://app:hunter2@db.example.com/shop" } });
    expect(url.value).toContain("hunter2");

    fireEvent.blur(url);
    expect(url.value).toBe("postgres://app:••••@db.example.com/shop");
    expect(document.body.textContent).not.toContain("hunter2");
    expect((screen.getByLabelText("Password") as HTMLInputElement).type).toBe("password");
  });

  it("tests with what was typed and reports the result without the password", async () => {
    const url = await openDialog();
    mocked.testConnection.mockResolvedValue({
      engine: "postgres",
      serverVersion: "PostgreSQL 16.2",
      location: "postgres://app:••••@db.example.com:5432/shop",
      tableCount: 12,
      latencyMs: 34,
    });

    fireEvent.focus(url);
    fireEvent.change(url, { target: { value: "postgres://app@db.example.com/shop" } });
    fireEvent.blur(url);
    fireEvent.change(screen.getByLabelText("Password"), { target: { value: "hunter2" } });
    fireEvent.click(screen.getByRole("button", { name: "Test connection" }));

    expect(await screen.findByText("Connection works")).toBeTruthy();
    expect(mocked.testConnection).toHaveBeenCalledWith({
      kind: "postgres",
      url: "postgres://app@db.example.com/shop",
      password: "hunter2",
    });
    // Testing does not connect or remember anything.
    expect(mocked.connect).not.toHaveBeenCalled();
    expect(screen.getByText(/PostgreSQL 16\.2 · 12 tables · 34 ms/)).toBeTruthy();
  });

  it("explains a failed connection and stays open", async () => {
    const url = await openDialog();
    mocked.connect.mockRejectedValue({
      kind: "auth_failed",
      message: "The server rejected the user name or password.",
      hint: "Check the credentials.",
    });

    fireEvent.focus(url);
    fireEvent.change(url, { target: { value: "postgres://app@db.example.com/shop" } });
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Sign-in rejected")).toBeTruthy();
    expect(within(alert).getByText("Check the credentials.")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
