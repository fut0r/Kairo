import type { ReactNode } from "react";

// A small stroke icon set, drawn to match the cylinder mark: 1.6px strokes
// with round caps on a 16px grid. Icons are decorative; controls that use one
// carry their own text or aria-label.

const PATHS = {
  chevron: <path d="M6 3.5 10.5 8 6 12.5" />,
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  play: <path d="M5 3.5v9l7.5-4.5z" />,
  refresh: (
    <>
      <path d="M13 8a5 5 0 1 1-1.6-3.7" />
      <path d="M13 2.5V5h-2.5" />
    </>
  ),
  copy: (
    <>
      <rect x="5.5" y="5.5" width="8" height="8" />
      <path d="M10.5 5.5v-3h-8v8h3" />
    </>
  ),
  folder: <path d="M2 4.5h4.2l1.3 1.5H14v7H2z" />,
  file: (
    <>
      <path d="M4 2h5.5L12 4.5V14H4z" />
      <path d="M9.5 2v2.5H12" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  search: (
    <>
      <circle cx="7" cy="7" r="4" />
      <path d="M10 10l3.5 3.5" />
    </>
  ),
  table: (
    <>
      <rect x="2.5" y="3" width="11" height="10" />
      <path d="M2.5 6.5h11M6.5 6.5V13" />
    </>
  ),
  view: (
    <>
      <path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z" />
      <circle cx="8" cy="8" r="1.8" />
    </>
  ),
  plug: (
    <>
      <path d="M6 2v3M10 2v3M4 5h8v2.5a4 4 0 0 1-8 0z" />
      <path d="M8 11.5V14" />
    </>
  ),
  save: (
    <>
      <path d="M3 2.5h8.5L13 4v9.5H3z" />
      <path d="M5.5 2.5v3h5v-3M5.5 13.5v-4h5v4" />
    </>
  ),
  first: <path d="M11.5 3.5 7 8l4.5 4.5M4.5 3.5v9" />,
  prev: <path d="M10 3.5 5.5 8l4.5 4.5" />,
  next: <path d="M6 3.5 10.5 8 6 12.5" />,
  last: <path d="M4.5 3.5 9 8l-4.5 4.5M11.5 3.5v9" />,
  warning: (
    <>
      <path d="M8 2.5 14 13H2z" />
      <path d="M8 6.5v3M8 11.2v.3" />
    </>
  ),
  check: <path d="M3 8.5l3.2 3L13 4.5" />,
  info: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 7.2v4M8 4.8v.3" />
    </>
  ),
  trash: <path d="M3 4.5h10M6 4.5V3h4v1.5M4.5 4.5l.5 9h6l.5-9" />,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}

/** The KairoDB mark: the cylinder from the website. */
export function LogoMark({ size = 22 }: { size?: number }) {
  return (
    <svg
      className="brand-mark"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <ellipse cx="12" cy="5" rx="7" ry="2.5" />
      <path d="M5 5v5c0 1.38 3.13 2.5 7 2.5s7-1.12 7-2.5V5" />
      <path d="M5 10v5c0 1.38 3.13 2.5 7 2.5s7-1.12 7-2.5v-5" />
      <path d="M5 15v4c0 1.38 3.13 2.5 7 2.5s7-1.12 7-2.5v-4" />
    </svg>
  );
}
