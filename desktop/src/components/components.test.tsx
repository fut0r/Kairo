import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { KairoError, toKairoError } from "../api/client";
import type { Cell, ResultColumn } from "../api/types";
import { CodeEditor } from "./CodeEditor";
import { DataGrid } from "./DataGrid";
import { Modal } from "./Modal";
import { ErrorNotice } from "./Notice";

describe("Modal", () => {
  function Example({ onClose }: { onClose: () => void }) {
    return (
      <Modal
        title="Delete everything?"
        onClose={onClose}
        footer={
          <>
            <button type="button" data-autofocus>
              Cancel
            </button>
            <button type="button">Delete</button>
          </>
        }
      >
        <p>Body</p>
      </Modal>
    );
  }

  it("is a labelled dialog that puts focus on the safe choice", () => {
    render(<Example onClose={() => {}} />);
    const dialog = screen.getByRole("dialog", { name: "Delete everything?" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
  });

  it("closes on Escape and on a click outside, but not on a click inside", () => {
    const onClose = vi.fn();
    render(<Example onClose={onClose} />);
    const dialog = screen.getByRole("dialog");

    fireEvent.mouseDown(screen.getByText("Body"));
    expect(onClose).not.toHaveBeenCalled();

    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.mouseDown(dialog.parentElement!);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("keeps Tab inside the dialog", () => {
    render(<Example onClose={() => {}} />);
    const dialog = screen.getByRole("dialog");
    const close = screen.getByRole("button", { name: "Close" });
    const last = screen.getByRole("button", { name: "Delete" });

    last.focus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(close);

    fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it("returns focus to where it was when it closes", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    const { unmount } = render(<Example onClose={() => {}} />);
    expect(document.activeElement).not.toBe(opener);
    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});

describe("DataGrid", () => {
  const columns: ResultColumn[] = [
    { name: "id", dataType: "INTEGER" },
    { name: "name", dataType: "TEXT" },
    { name: "note", dataType: "" },
  ];
  const rows: Cell[][] = [
    [{ kind: "int", text: "1" }, { kind: "text", text: "alice" }, { kind: "null" }],
    [{ kind: "int", text: "9007199254740993" }, { kind: "text" }, { kind: "blob", text: "<blob 4 bytes>" }],
    [{ kind: "int", text: "3" }, { kind: "text", text: "x".repeat(500), truncated: true }, { kind: "bool", text: "true" }],
  ];

  it("renders each kind of value distinctly and keeps large integers exact", () => {
    render(<DataGrid label="Rows of users" columns={columns} rows={rows} />);
    const grid = screen.getByRole("grid", { name: "Rows of users" });

    const nullCell = within(grid).getByText("NULL");
    expect(nullCell.className).toContain("cell-null");
    expect(within(grid).getByText("9007199254740993").className).toContain("cell-num");
    expect(within(grid).getByText("<blob 4 bytes>").className).toContain("cell-blob");
    // Missing text is an empty string, not the word "undefined".
    expect(within(grid).queryByText("undefined")).toBeNull();
  });

  it("numbers rows from the page offset", () => {
    render(<DataGrid label="g" columns={columns} rows={rows} rowOffset={100} />);
    expect(screen.getByRole("rowheader", { name: "101" })).toBeTruthy();
    expect(screen.getByRole("rowheader", { name: "103" })).toBeTruthy();
  });

  it("moves the active cell with the keyboard and shows its full value", () => {
    render(<DataGrid label="g" columns={columns} rows={rows} />);
    const grid = screen.getByRole("grid");

    // The first key press selects the first cell.
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    const first = document.getElementById(grid.getAttribute("aria-activedescendant")!)!;
    expect(first.textContent).toBe("1");

    fireEvent.keyDown(grid, { key: "ArrowRight" });
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    const active = document.getElementById(grid.getAttribute("aria-activedescendant")!)!;
    expect(active.getAttribute("aria-selected")).toBe("true");
    // The cell shows a preview; the detail strip shows everything that was sent.
    expect(active.textContent!.length).toBeLessThan(260);
    expect(screen.getByText("x".repeat(500))).toBeTruthy();
    expect(screen.getByText(/long value/)).toBeTruthy();

    // Movement stops at the edges.
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    fireEvent.keyDown(grid, { key: "End" });
    fireEvent.keyDown(grid, { key: "ArrowRight" });
    expect(document.getElementById(grid.getAttribute("aria-activedescendant")!)!.textContent).toBe("true");
  });

  it("offers sorting only when asked to, and reports the sorted column", () => {
    const onSort = vi.fn();
    const { rerender } = render(<DataGrid label="g" columns={columns} rows={rows} />);
    expect(screen.queryByRole("button", { name: /name/ })).toBeNull();

    rerender(
      <DataGrid
        label="g"
        columns={columns}
        rows={rows}
        onSort={onSort}
        sort={{ column: "name", descending: true }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /^id/ }));
    expect(onSort).toHaveBeenCalledWith("id");
    expect(screen.getByRole("columnheader", { name: /name/ }).getAttribute("aria-sort")).toBe("descending");
  });
});

describe("CodeEditor", () => {
  it("is a labelled text field whose value is the source", () => {
    const onChange = vi.fn();
    render(<CodeEditor label="SQL query" language="sql" value="SELECT 1" onChange={onChange} />);
    const field = screen.getByRole("textbox", { name: "SQL query" }) as HTMLTextAreaElement;
    expect(field.value).toBe("SELECT 1");
    fireEvent.change(field, { target: { value: "SELECT 2" } });
    expect(onChange).toHaveBeenCalledWith("SELECT 2");
  });

  it("runs on Ctrl+Enter and saves on Ctrl+S", () => {
    const onRun = vi.fn();
    const onSave = vi.fn();
    render(
      <CodeEditor label="e" language="sql" value="x" onChange={() => {}} onRun={onRun} onSave={onSave} />,
    );
    const field = screen.getByRole("textbox");
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(field, { key: "Enter", metaKey: true });
    fireEvent.keyDown(field, { key: "s", ctrlKey: true });
    expect(onRun).toHaveBeenCalledTimes(2);
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("indents on Tab, and lets Tab through after Escape so focus can leave", () => {
    render(<CodeEditor label="e" language="kairo" value="" onChange={() => {}} />);
    const field = screen.getByRole("textbox");

    // fireEvent returns false when the handler called preventDefault.
    expect(fireEvent.keyDown(field, { key: "Tab" })).toBe(false);
    fireEvent.keyDown(field, { key: "Escape" });
    expect(fireEvent.keyDown(field, { key: "Tab" })).toBe(true);
    expect(fireEvent.keyDown(field, { key: "Tab" })).toBe(false);
  });

  it("marks lines with problems and flags the field as invalid", () => {
    const { container } = render(
      <CodeEditor
        label="e"
        language="kairo"
        value={"table t {\n  name string\n}"}
        onChange={() => {}}
        diagnostics={[
          { severity: "error", message: "Expected `:`", line: 2, column: 3, endLine: 2, endColumn: 14 },
        ]}
      />,
    );
    expect(screen.getByRole("textbox").getAttribute("aria-invalid")).toBe("true");
    expect(container.querySelector(".editor-gutter .has-error")?.textContent).toBe("2");
    expect(container.querySelectorAll(".squiggle")).toHaveLength(1);
  });
});

describe("errors", () => {
  it("normalises whatever a command rejects with", () => {
    const structured = toKairoError({ kind: "auth_failed", message: "no", hint: "retry" });
    expect(structured).toBeInstanceOf(KairoError);
    expect(structured.kind).toBe("auth_failed");
    expect(structured.hint).toBe("retry");

    const panic = toKairoError("thread panicked");
    expect(panic.kind).toBe("internal");
    expect(panic.detail).toBe("thread panicked");

    expect(toKairoError(structured)).toBe(structured);
  });

  it("shows the heading for the kind, then message, detail and hint", () => {
    render(
      <ErrorNotice
        error={
          new KairoError({
            kind: "network",
            message: "The server refused the connection.",
            detail: "os error 10061",
            hint: "Check the host and port.",
          })
        }
      />,
    );
    const alert = screen.getByRole("alert");
    expect(within(alert).getByText("Can't reach the server")).toBeTruthy();
    expect(within(alert).getByText("The server refused the connection.")).toBeTruthy();
    expect(within(alert).getByText("os error 10061")).toBeTruthy();
    expect(within(alert).getByText("Check the host and port.")).toBeTruthy();
  });
});
