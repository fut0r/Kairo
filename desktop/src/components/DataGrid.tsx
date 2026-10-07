import { memo, useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import type { Cell, ResultColumn, SortSpec } from "../api/types";
import { copyText } from "../lib/clipboard";
import { CopyButton } from "./Controls";

/** Cells show at most this much text; the detail strip shows the rest. */
const CELL_PREVIEW = 240;
const PAGE_JUMP = 15;

interface DataGridProps {
  /** Names the grid for assistive technology, e.g. "Rows of users". */
  label: string;
  columns: ResultColumn[];
  rows: Cell[][];
  /** Number of the first row, so numbering continues across pages. */
  rowOffset?: number;
  sort?: SortSpec | null;
  /** Makes column headers sortable. */
  onSort?: (column: string) => void;
}

interface Position {
  row: number;
  col: number;
}

function cellText(cell: Cell): string {
  return cell.kind === "null" ? "NULL" : (cell.text ?? "");
}

function cellClass(cell: Cell): string | undefined {
  switch (cell.kind) {
    case "null":
      return "cell-null";
    case "int":
    case "float":
      return "cell-num";
    case "bool":
      return "cell-bool";
    case "blob":
      return "cell-blob";
    default:
      return undefined;
  }
}

interface RowProps {
  gridId: string;
  row: Cell[];
  index: number;
  number: number;
  activeCol: number | null;
  onSelect: (position: Position) => void;
}

// Memoised so that moving the active cell re-renders two rows, not all of them.
const GridRow = memo(function GridRow({ gridId, row, index, number, activeCol, onSelect }: RowProps) {
  return (
    <tr role="row" aria-rowindex={index + 2}>
      <td className="row-num" role="rowheader">
        {number}
      </td>
      {row.map((cell, col) => {
        const text = cellText(cell);
        const shown = text.length > CELL_PREVIEW ? `${text.slice(0, CELL_PREVIEW)}…` : text;
        const active = activeCol === col;
        const classes = [cellClass(cell), active ? "cell-active" : undefined].filter(Boolean).join(" ");
        return (
          <td
            key={col}
            id={`${gridId}-r${index}-c${col}`}
            role="gridcell"
            aria-selected={active}
            className={classes || undefined}
            onMouseDown={() => onSelect({ row: index, col })}
          >
            {/* <bdi> lets Arabic or Hebrew read in its own direction while the
                cell stays aligned with the rest of its column. */}
            {cell.kind === "text" ? <bdi>{shown}</bdi> : shown}
          </td>
        );
      })}
    </tr>
  );
});

/**
 * A read-only result grid. Arrow keys move between cells, Home/End and
 * PageUp/PageDown jump, and Ctrl/Cmd+C copies the active cell.
 */
export function DataGrid({ label, columns, rows, rowOffset = 0, sort, onSort }: DataGridProps) {
  const gridId = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<Position | null>(null);

  // A new result set invalidates the old position.
  useEffect(() => {
    setActive(null);
  }, [rows, columns]);

  useEffect(() => {
    if (!active) return;
    document
      .getElementById(`${gridId}-r${active.row}-c${active.col}`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, gridId]);

  const onKeyDown = (event: KeyboardEvent<HTMLTableElement>) => {
    if (rows.length === 0 || columns.length === 0) return;
    const current = active ?? { row: 0, col: 0 };
    const lastRow = rows.length - 1;
    const lastCol = columns.length - 1;
    let next: Position | null = null;

    switch (event.key) {
      case "ArrowDown":
        next = { row: Math.min(lastRow, current.row + 1), col: current.col };
        break;
      case "ArrowUp":
        next = { row: Math.max(0, current.row - 1), col: current.col };
        break;
      case "ArrowRight":
        next = { row: current.row, col: Math.min(lastCol, current.col + 1) };
        break;
      case "ArrowLeft":
        next = { row: current.row, col: Math.max(0, current.col - 1) };
        break;
      case "PageDown":
        next = { row: Math.min(lastRow, current.row + PAGE_JUMP), col: current.col };
        break;
      case "PageUp":
        next = { row: Math.max(0, current.row - PAGE_JUMP), col: current.col };
        break;
      case "Home":
        next = event.ctrlKey || event.metaKey ? { row: 0, col: 0 } : { row: current.row, col: 0 };
        break;
      case "End":
        next =
          event.ctrlKey || event.metaKey
            ? { row: lastRow, col: lastCol }
            : { row: current.row, col: lastCol };
        break;
      case "c":
      case "C":
        if ((event.ctrlKey || event.metaKey) && active && !window.getSelection()?.toString()) {
          const cell = rows[active.row]?.[active.col];
          if (cell) void copyText(cellText(cell));
          event.preventDefault();
        }
        return;
      default:
        return;
    }

    event.preventDefault();
    // The first key press lands on the first cell rather than skipping it.
    setActive(active ? next : current);
  };

  const activeCell = active ? rows[active.row]?.[active.col] : undefined;
  const activeColumn = active ? columns[active.col] : undefined;

  return (
    <div className="fill">
      <div className="grid-wrap fill" ref={wrap}>
        <table
          className="grid"
          role="grid"
          aria-label={label}
          aria-rowcount={rows.length + 1}
          aria-colcount={columns.length + 1}
          aria-activedescendant={active ? `${gridId}-r${active.row}-c${active.col}` : undefined}
          tabIndex={0}
          onKeyDown={onKeyDown}
        >
          <thead>
            <tr role="row" aria-rowindex={1}>
              <th className="row-num" role="columnheader">
                <span className="visually-hidden">Row</span>#
              </th>
              {columns.map((column) => {
                const sorted = sort?.column === column.name;
                const content = (
                  <>
                    <span className="truncate">{column.name}</span>
                    {column.dataType && <span className="col-type">{column.dataType.toLowerCase()}</span>}
                    {sorted && <span className="sort-mark">{sort?.descending ? "▼" : "▲"}</span>}
                  </>
                );
                return (
                  <th
                    key={column.name}
                    role="columnheader"
                    aria-sort={sorted ? (sort?.descending ? "descending" : "ascending") : undefined}
                  >
                    {onSort ? (
                      <button
                        type="button"
                        className="col-head"
                        onClick={() => onSort(column.name)}
                        title={`Sort by ${column.name}`}
                      >
                        {content}
                      </button>
                    ) : (
                      <span className="col-head">{content}</span>
                    )}
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => (
              <GridRow
                key={index}
                gridId={gridId}
                row={row}
                index={index}
                number={rowOffset + index + 1}
                activeCol={active?.row === index ? active.col : null}
                onSelect={setActive}
              />
            ))}
          </tbody>
        </table>
      </div>

      {activeCell && activeColumn && (
        <div className="cell-detail" aria-live="polite">
          <div>
            <div className="muted">
              {activeColumn.name}
              {activeColumn.dataType ? ` · ${activeColumn.dataType.toLowerCase()}` : ""} · row{" "}
              {rowOffset + (active?.row ?? 0) + 1}
              {activeCell.truncated ? " · long value, showing the first 2,000 characters" : ""}
            </div>
            <pre dir="auto">{cellText(activeCell)}</pre>
          </div>
          <CopyButton text={cellText(activeCell)} label="Copy value" />
        </div>
      )}
    </div>
  );
}
