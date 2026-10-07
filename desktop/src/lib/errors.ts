import { toKairoError, type KairoError } from "../api/client";
import type { ErrorKind } from "../api/types";

const TITLES: Record<ErrorKind, string> = {
  invalid_input: "That can't be used",
  not_found: "Not found",
  invalid_database: "Not a database",
  invalid_url: "Invalid connection URL",
  auth_failed: "Sign-in rejected",
  network: "Can't reach the server",
  tls: "Secure connection failed",
  timeout: "Timed out",
  busy: "Database is busy",
  permission_denied: "Not allowed",
  syntax: "SQL error",
  constraint: "Constraint violated",
  schema: "Schema error",
  unsupported: "Not supported",
  confirmation_required: "Confirmation needed",
  not_connected: "Not connected",
  io: "File error",
  database: "Database error",
  internal: "Unexpected error",
};

/** A short heading for an error, chosen from its kind rather than its text. */
export function errorTitle(error: KairoError): string {
  return TITLES[error.kind] ?? TITLES.internal;
}

/** One line for the activity log. */
export function errorSummary(raw: unknown): string {
  const error = toKairoError(raw);
  return `${errorTitle(error)}: ${error.message}`;
}

export { toKairoError };
