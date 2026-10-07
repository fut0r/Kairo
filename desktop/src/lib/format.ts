const integer = new Intl.NumberFormat("en-US");

/** 12345 → "12,345". */
export function formatCount(value: number): string {
  return integer.format(value);
}

/** "1 row", "2 rows". */
export function plural(count: number, noun: string): string {
  return `${formatCount(count)} ${noun}${count === 1 ? "" : "s"}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export function formatDuration(ms: number): string {
  if (ms < 1) return "<1 ms";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)} s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes} min ${seconds} s`;
}

/** A Unix timestamp in seconds as "just now", "5 min ago", or a date. */
export function formatAgo(seconds: number, now: number = Date.now()): string {
  const elapsed = Math.max(0, Math.floor(now / 1000) - seconds);
  if (elapsed < 45) return "just now";
  if (elapsed < 3600) return `${Math.round(elapsed / 60)} min ago`;
  if (elapsed < 86_400) return `${Math.round(elapsed / 3600)} h ago`;
  if (elapsed < 7 * 86_400) return `${Math.round(elapsed / 86_400)} d ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}

export function formatClock(date: Date): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** The 1-based row range a page shows: "51–100". */
export function pageRange(offset: number, shown: number): string {
  if (shown === 0) return "0";
  return `${formatCount(offset + 1)}–${formatCount(offset + shown)}`;
}

/** The last path segment, for either separator. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}
