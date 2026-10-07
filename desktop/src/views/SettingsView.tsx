import { useState, type ReactNode } from "react";
import type { Theme } from "../api/types";
import { CopyButton } from "../components/Controls";
import { Modal } from "../components/Modal";
import { useApp } from "../state/app";

const THEMES: { id: Theme; label: string }[] = [
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
  { id: "system", label: "Match system" },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = `settings-${title.toLowerCase().replace(/\s+/g, "-")}`;
  return (
    <section className="stack" aria-labelledby={id}>
      <h2 id={id}>{title}</h2>
      <div className="hairline-grid">{children}</div>
    </section>
  );
}

function Row({ children }: { children: ReactNode }) {
  return <div className="card stack" style={{ gap: "var(--space-2)" }}>{children}</div>;
}

interface NumberSettingProps {
  id: string;
  label: string;
  hint: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onCommit: (value: number) => void;
}

/** A number field that saves when the user is done with it, not on every key. */
function NumberSetting({ id, label, hint, value, min, max, step = 1, onCommit }: NumberSettingProps) {
  const [draft, setDraft] = useState<string | null>(null);

  const commit = () => {
    if (draft === null) return;
    const parsed = Number(draft);
    setDraft(null);
    if (Number.isFinite(parsed)) onCommit(Math.min(max, Math.max(min, Math.round(parsed))));
  };

  return (
    <Row>
      <label className="label" htmlFor={id}>
        {label}
      </label>
      <input
        id={id}
        className="input"
        style={{ maxWidth: 160 }}
        type="number"
        min={min}
        max={max}
        step={step}
        value={draft ?? value}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
        }}
        aria-describedby={`${id}-hint`}
      />
      <p className="hint" id={`${id}-hint`}>
        {hint} Between {min.toLocaleString("en-US")} and {max.toLocaleString("en-US")}.
      </p>
    </Row>
  );
}

export function SettingsView() {
  const app = useApp();
  const { settings, updateSettings, info } = app;
  const [confirmClear, setConfirmClear] = useState(false);
  const modifier = info?.platform === "macos" ? "⌘" : "Ctrl";

  // Safe to paste into a bug report: locations are already masked, and no
  // query text or data is included.
  const diagnostics = [
    `KairoDB ${info?.version ?? "?"} (core ${info?.coreVersion ?? "?"})`,
    `Platform: ${info?.platform ?? "?"} ${info?.arch ?? ""}`.trim(),
    `Theme: ${settings.theme}`,
    `Open connections: ${app.connections.length}`,
    ...app.connections.map((c) => `  - ${c.engine} · ${c.serverVersion}${c.readOnly ? " · read-only" : ""}`),
  ].join("\n");

  return (
    <div className="page-pad page-narrow stack-lg">
      <Section title="Appearance">
        <Row>
          <span className="label" id="theme-label">
            Theme
          </span>
          <div className="segmented" role="radiogroup" aria-labelledby="theme-label">
            {THEMES.map((theme) => (
              <button
                key={theme.id}
                type="button"
                role="radio"
                aria-checked={settings.theme === theme.id}
                onClick={() => void updateSettings({ theme: theme.id })}
              >
                {theme.label}
              </button>
            ))}
          </div>
        </Row>
      </Section>

      <Section title="Safety">
        <Row>
          <label className="check-row">
            <input
              type="checkbox"
              checked={settings.confirmWrites}
              onChange={(event) => void updateSettings({ confirmWrites: event.target.checked })}
            />
            <span>
              <span className="label">Ask before statements that change data</span>
              <span className="hint" style={{ display: "block" }}>
                Covers INSERT, CREATE and similar. Statements that can remove or overwrite data,
                such as DROP, DELETE, UPDATE and ALTER, always ask, whatever this is set to.
              </span>
            </span>
          </label>
        </Row>
        <NumberSetting
          id="setting-timeout"
          label="Query time limit (seconds)"
          hint="A statement still running after this long is stopped."
          value={settings.queryTimeoutSecs}
          min={1}
          max={3600}
          onCommit={(queryTimeoutSecs) => void updateSettings({ queryTimeoutSecs })}
        />
      </Section>

      <Section title="Data">
        <NumberSetting
          id="setting-page-size"
          label="Rows per page in the Explorer"
          hint="The default; each table can be changed from its toolbar."
          value={settings.pageSize}
          min={10}
          max={1000}
          step={10}
          onCommit={(pageSize) => void updateSettings({ pageSize })}
        />
        <NumberSetting
          id="setting-max-rows"
          label="Rows kept from a query"
          hint="Results beyond this are not loaded, and the result says so."
          value={settings.maxRows}
          min={10}
          max={50000}
          step={100}
          onCommit={(maxRows) => void updateSettings({ maxRows })}
        />
      </Section>

      <Section title="Privacy">
        <Row>
          <p>
            Kairo keeps settings, recent connections and query history in one file on this machine.
            It has no account, no telemetry, and makes no network requests of its own.{" "}
            <strong>Passwords are never written to it.</strong> A saved PostgreSQL connection
            remembers the host, port, database and user, and asks for the password each time.
          </p>
          {info?.storePath && (
            <p className="mono muted selectable" style={{ overflowWrap: "anywhere" }}>
              {info.storePath}
            </p>
          )}
          <div>
            <button type="button" className="btn btn-outline" onClick={() => setConfirmClear(true)}>
              Clear recent connections and history
            </button>
          </div>
        </Row>
      </Section>

      <Section title="Keyboard">
        <Row>
          <dl className="kv">
            <dt>
              <kbd>{modifier}</kbd> <kbd>1</kbd> … <kbd>6</kbd>
            </dt>
            <dd>Switch between Overview, Explorer, Query, Schema, Export and Settings</dd>
            <dt>
              <kbd>{modifier}</kbd> <kbd>O</kbd>
            </dt>
            <dd>Open a database</dd>
            <dt>
              <kbd>{modifier}</kbd> <kbd>Enter</kbd>
            </dt>
            <dd>Run the query</dd>
            <dt>
              <kbd>{modifier}</kbd> <kbd>S</kbd>
            </dt>
            <dd>Save the schema</dd>
            <dt>
              <kbd>Esc</kbd> then <kbd>Tab</kbd>
            </dt>
            <dd>Leave a code editor (Tab alone indents)</dd>
            <dt>Arrow keys</dt>
            <dd>
              Move between cells in a result grid. <kbd>{modifier}</kbd> <kbd>C</kbd> copies the
              active cell
            </dd>
          </dl>
        </Row>
      </Section>

      <Section title="About">
        <Row>
          <dl className="kv">
            <dt>Version</dt>
            <dd className="mono">
              {info?.version ?? "—"} (core {info?.coreVersion ?? "—"})
            </dd>
            <dt>Platform</dt>
            <dd className="mono">
              {info?.platform ?? "—"} {info?.arch ?? ""}
            </dd>
            <dt>License</dt>
            <dd>GNU GPL 3.0. KairoDB is free software and comes with no warranty.</dd>
            <dt>Website</dt>
            <dd className="mono">https://kairo.arabdev.site</dd>
            <dt>Source</dt>
            <dd className="mono">https://github.com/fut0r/Kairo</dd>
          </dl>
          <div>
            <CopyButton text={diagnostics} label="Copy diagnostics" />
          </div>
        </Row>
      </Section>

      {confirmClear && (
        <Modal
          title="Clear recent connections and history?"
          onClose={() => setConfirmClear(false)}
          footer={
            <>
              <button type="button" className="btn btn-outline" onClick={() => setConfirmClear(false)} data-autofocus>
                Cancel
              </button>
              <button
                type="button"
                className="btn btn-danger"
                onClick={() => {
                  setConfirmClear(false);
                  void app.clearRecents();
                }}
              >
                Clear both
              </button>
            </>
          }
        >
          <p>
            This removes the Recent list and the saved query history for every database. Your
            databases and schema files are not touched, and open connections stay open.
          </p>
        </Modal>
      )}
    </div>
  );
}
