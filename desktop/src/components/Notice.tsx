import type { ReactNode } from "react";
import type { KairoError } from "../api/client";
import { errorTitle } from "../lib/errors";

type Tone = "error" | "success" | "warning" | "info";

interface NoticeProps {
  tone: Tone;
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}

/** An inline message. Errors are announced to assistive technology at once. */
export function Notice({ tone, title, children, action }: NoticeProps) {
  return (
    <div className={`notice notice-${tone}`} role={tone === "error" ? "alert" : "status"}>
      <div className="notice-body">
        <div className="notice-title">{title}</div>
        {children}
      </div>
      {action}
    </div>
  );
}

/** A failed command, shown by kind: heading, message, driver detail, next step. */
export function ErrorNotice({ error, action }: { error: KairoError; action?: ReactNode }) {
  return (
    <Notice tone="error" title={errorTitle(error)} action={action}>
      <div className="selectable">{error.message}</div>
      {error.detail && <div className="notice-detail">{error.detail}</div>}
      {error.hint && <div className="notice-hint">{error.hint}</div>}
    </Notice>
  );
}

export function Loading({ label }: { label: string }) {
  return (
    <div className="loading-row" role="status">
      <span className="spinner" aria-hidden="true" />
      {label}
    </div>
  );
}
