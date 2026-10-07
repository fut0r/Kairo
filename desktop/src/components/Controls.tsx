import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { copyText } from "../lib/clipboard";
import { tokenize, type Language } from "../lib/highlight";
import { Icon } from "./Icon";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  count?: number;
}

interface TabsProps<T extends string> {
  label: string;
  items: TabItem<T>[];
  value: T;
  onChange: (id: T) => void;
  /** Prefix for the ids that link each tab to its panel. */
  idPrefix: string;
}

/** A tab list with arrow-key movement. Pair each tab with `tabPanelProps`. */
export function Tabs<T extends string>({ label, items, value, onChange, idPrefix }: TabsProps<T>) {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = items.findIndex((item) => item.id === value);
    let next = index;
    if (event.key === "ArrowRight") next = (index + 1) % items.length;
    else if (event.key === "ArrowLeft") next = (index - 1 + items.length) % items.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;
    else return;

    event.preventDefault();
    const target = items[next];
    if (!target) return;
    onChange(target.id);
    document.getElementById(`${idPrefix}-tab-${target.id}`)?.focus();
  };

  return (
    <div className="tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="tab"
          id={`${idPrefix}-tab-${item.id}`}
          className="tab"
          aria-selected={item.id === value}
          aria-controls={`${idPrefix}-panel-${item.id}`}
          tabIndex={item.id === value ? 0 : -1}
          onClick={() => onChange(item.id)}
        >
          {item.label}
          {item.count !== undefined && <span className="count">{item.count}</span>}
        </button>
      ))}
    </div>
  );
}

export function tabPanelProps(idPrefix: string, id: string) {
  return {
    role: "tabpanel" as const,
    id: `${idPrefix}-panel-${id}`,
    "aria-labelledby": `${idPrefix}-tab-${id}`,
  };
}

/** A button that copies text and briefly confirms it. */
export function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  return (
    <button
      type="button"
      className="btn btn-ghost btn-sm"
      onClick={async () => {
        if (await copyText(text)) {
          setCopied(true);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1400);
        }
      }}
    >
      <Icon name={copied ? "check" : "copy"} size={14} />
      <span aria-live="polite">{copied ? "Copied" : label}</span>
    </button>
  );
}

/** Read-only code with the same colouring as the editors. */
export function CodeBlock({
  code,
  language,
  wrap,
}: {
  code: string;
  language: Language | "text";
  wrap?: boolean;
}) {
  const tokens = language === "text" ? [{ kind: "plain" as const, text: code }] : tokenize(code, language);
  return (
    <pre className={wrap ? "code-block code-wrap" : "code-block"} tabIndex={0}>
      <code>
        {tokens.map((token, index) =>
          token.kind === "plain" ? (
            token.text
          ) : (
            <span key={index} className={`tok-${token.kind}`}>
              {token.text}
            </span>
          ),
        )}
      </code>
    </pre>
  );
}

/** A titled code surface, in the style of the website's terminal windows. */
export function Window({
  title,
  actions,
  children,
  className,
}: {
  title: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className ? `window ${className}` : "window"}>
      <header className="window-header">
        <span className="window-title truncate">{title}</span>
        <span className="spacer" />
        {actions}
      </header>
      {children}
    </section>
  );
}

/** A one-line command a user could run in a terminal to do the same thing. */
export function CliHint({ command }: { command: string }) {
  return (
    <div className="row row-wrap">
      <span className="hint">Same thing from a terminal:</span>
      <code className="code-chip">{command}</code>
      <CopyButton text={command} />
    </div>
  );
}
