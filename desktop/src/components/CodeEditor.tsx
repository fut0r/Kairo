import {
  forwardRef,
  useId,
  useImperativeHandle,
  useMemo,
  useRef,
  type KeyboardEvent,
} from "react";
import type { Diagnostic } from "../api/types";
import { tokenize, type Language } from "../lib/highlight";

const INDENT = "  ";

export interface CodeEditorHandle {
  focus: () => void;
  /** Moves the caret to a 1-based line and column. */
  goTo: (line: number, column: number) => void;
}

interface CodeEditorProps {
  label: string;
  language: Language;
  value: string;
  onChange: (value: string) => void;
  diagnostics?: Diagnostic[];
  /** Ctrl/Cmd+Enter. */
  onRun?: () => void;
  /** Ctrl/Cmd+S. */
  onSave?: () => void;
  placeholder?: string;
}

/**
 * A small code editor: a transparent textarea over a coloured copy of its
 * text. The textarea is the real control, so selection, undo, IME input and
 * screen readers all behave as they do in any text field.
 *
 * Tab indents. To move focus out with the keyboard, press Escape, then Tab.
 */
export const CodeEditor = forwardRef<CodeEditorHandle, CodeEditorProps>(function CodeEditor(
  { label, language, value, onChange, diagnostics = [], onRun, onSave, placeholder },
  ref,
) {
  const input = useRef<HTMLTextAreaElement>(null);
  const releaseTab = useRef(false);
  const hintId = useId();

  useImperativeHandle(ref, () => ({
    focus: () => input.current?.focus(),
    goTo: (line, column) => {
      const field = input.current;
      if (!field) return;
      const lines = field.value.split("\n");
      let offset = 0;
      for (let i = 0; i < Math.min(line - 1, lines.length); i += 1) {
        offset += (lines[i]?.length ?? 0) + 1;
      }
      offset += Math.max(0, column - 1);
      field.focus();
      field.setSelectionRange(offset, offset);
    },
  }));

  const tokens = useMemo(() => tokenize(value, language), [value, language]);
  const lineCount = useMemo(() => value.split("\n").length, [value]);

  const worstByLine = useMemo(() => {
    const worst = new Map<number, Diagnostic["severity"]>();
    for (const diagnostic of diagnostics) {
      if (diagnostic.severity === "error" || !worst.has(diagnostic.line)) {
        worst.set(diagnostic.line, diagnostic.severity);
      }
    }
    return worst;
  }, [diagnostics]);

  /** Replaces the selection through the browser, so the edit can be undone. */
  const insert = (text: string) => {
    const field = input.current;
    if (!field) return;
    field.focus();
    if (!document.execCommand?.("insertText", false, text)) {
      const { selectionStart, selectionEnd } = field;
      field.setRangeText(text, selectionStart, selectionEnd, "end");
      onChange(field.value);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const modifier = event.ctrlKey || event.metaKey;

    if (modifier && event.key === "Enter" && onRun) {
      event.preventDefault();
      onRun();
      return;
    }
    if (modifier && event.key.toLowerCase() === "s" && onSave) {
      event.preventDefault();
      onSave();
      return;
    }
    if (event.key === "Escape") {
      // The next Tab leaves the editor instead of indenting.
      releaseTab.current = true;
      return;
    }
    if (event.key === "Tab" && !modifier && !event.altKey) {
      if (releaseTab.current) {
        releaseTab.current = false;
        return;
      }
      event.preventDefault();
      if (!event.shiftKey) insert(INDENT);
      return;
    }
    if (event.key === "Enter" && !modifier) {
      // Keep the indentation of the current line.
      const field = event.currentTarget;
      const before = field.value.slice(0, field.selectionStart);
      const line = before.slice(before.lastIndexOf("\n") + 1);
      const indent = /^[ \t]*/.exec(line)?.[0] ?? "";
      const opensBlock = line.trimEnd().endsWith("{");
      event.preventDefault();
      insert(`\n${indent}${opensBlock ? INDENT : ""}`);
      return;
    }
    releaseTab.current = false;
  };

  return (
    <div className="editor">
      <div className="editor-inner">
        <div className="editor-gutter" aria-hidden="true">
          {Array.from({ length: lineCount }, (_, index) => {
            const severity = worstByLine.get(index + 1);
            return (
              <div key={index} className={severity ? `has-${severity}` : undefined}>
                {index + 1}
              </div>
            );
          })}
        </div>

        <div className="editor-layers">
          <div className="editor-marks" aria-hidden="true">
            {diagnostics.map((diagnostic, index) => {
              // Positions are in characters; the monospace face makes one
              // character one `ch` wide.
              const sameLine = diagnostic.endLine === diagnostic.line;
              const width = sameLine ? Math.max(1, diagnostic.endColumn - diagnostic.column) : 1;
              return (
                <span
                  key={index}
                  className={diagnostic.severity === "error" ? "squiggle" : "squiggle warning"}
                  // Offsets start at the layer's padding edge, so the text
                  // padding (10px, 14px) and the 20px line height are added.
                  style={{
                    top: `${10 + (diagnostic.line - 1) * 20}px`,
                    left: `calc(14px + ${diagnostic.column - 1}ch)`,
                    width: `${width}ch`,
                  }}
                />
              );
            })}
          </div>

          <pre className="editor-highlight" aria-hidden="true">
            {tokens.map((token, index) =>
              token.kind === "plain" ? (
                token.text
              ) : (
                <span key={index} className={`tok-${token.kind}`}>
                  {token.text}
                </span>
              ),
            )}
            {/* A trailing newline needs something after it to take up a line. */}
            {"\n"}
          </pre>

          <textarea
            ref={input}
            className="editor-input"
            aria-label={label}
            aria-describedby={hintId}
            aria-invalid={diagnostics.some((d) => d.severity === "error") || undefined}
            value={value}
            placeholder={placeholder}
            onChange={(event) => onChange(event.target.value)}
            onKeyDown={onKeyDown}
            spellCheck={false}
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            wrap="off"
          />
        </div>
      </div>
      <span id={hintId} className="visually-hidden">
        Tab indents. Press Escape, then Tab, to leave the editor.
      </span>
    </div>
  );
});
