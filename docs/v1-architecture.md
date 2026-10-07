# KairoDB 1.0 architecture

This document records the audit of the 0.4.0 codebase and the plan that turned
it into 1.0.0: one Rust core used by two front ends, the existing CLI and a new
Tauri desktop app.

## 1. Audit of 0.4.0

The 0.4.0 tree was a single binary crate. `main.rs` held command routing and
most behaviour; `core/reader.rs` produced terminal-formatted strings directly.

| Area | Finding | Consequence for 1.0 |
| --- | --- | --- |
| Value decoding | Every cell was read with `try_get::<String>`; any non-text column fell back to `"null"` | `kairo query` printed `null` for integers. Replaced with typed decoding. |
| Credentials | `kairo read postgres://user:pass@…` echoed the full URL | Passwords are now masked everywhere they are displayed or logged. |
| `kairo tables` | Queried `sqlite_master` regardless of adapter | Failed on PostgreSQL. Now uses engine-specific introspection. |
| Schema apply | One multi-statement string sent as a prepared statement | Multi-table schemas failed on PostgreSQL. Statements now run one by one in a transaction. |
| String defaults | Emitted as `DEFAULT "text"` | Invalid on PostgreSQL. Now emitted as a quoted SQL literal. |
| SQLite paths | Built as `sqlite:{path}?mode=ro` | Paths containing spaces, `?` or `#` broke. Now passed as a filename, never as a URL. |
| Identifiers | Table names interpolated unquoted into SQL | Reserved words and unusual names failed. Introspection SQL now quotes or binds every identifier. |
| Export round trip | `export` wrote `[required]`, `float`, and raw SQL defaults that the grammar rejected | Exported files could not be applied. The grammar was extended and export only emits what the parser accepts. |
| TLS | sqlx built without a TLS backend | Hosted PostgreSQL was unreachable. rustls is now enabled. |
| Row counts | Decoded as `i32` | Overflowed above 2.1 billion rows. Now `i64`. |
| Layering | Formatting, SQL and I/O mixed in one function per command | Nothing was reusable from a GUI. |

## 2. Layers

```
                 ┌────────────────────┐   ┌──────────────────────────┐
  adapters (UI)  │ CLI  (src/)        │   │ Desktop (desktop/)       │
                 │ clap + colored     │   │ Tauri commands + React   │
                 └─────────┬──────────┘   └────────────┬─────────────┘
                           │                           │
                 ┌─────────▼───────────────────────────▼─────────────┐
  services       │ kairo_core::services                              │
  (use cases)    │ query · safety · schema_apply · export · report   │
                 │ project                                           │
                 └─────────┬───────────────────────────┬─────────────┘
                           │                           │
                 ┌─────────▼──────────┐   ┌────────────▼─────────────┐
  domain         │ kairo_core::schema │   │ kairo_core::db           │
                 │ grammar · parser   │   │ Connection · target      │
                 │ sqlgen · render    │   │ sqlite · postgres · value│
                 └────────────────────┘   └──────────────────────────┘
```

Rules that keep the layers honest:

- `kairo-core` never prints, never colours, and never returns terminal-formatted
  text except from `services::report`, which exists to keep `kairo read` output
  stable.
- Both adapters call the same service functions. The desktop app contains no
  schema parsing and no SQL generation; the TypeScript side only renders.
- Every public result type derives `Serialize` with `camelCase` field names.
  That serialisation is the wire contract mirrored in `desktop/src/api/types.ts`.
- Errors are a single `KairoError { kind, message, detail, hint }`. Its
  constructor passes every string through `redact`, so a credential cannot reach
  a caller through an error.

## 3. Repository layout

```
Cargo.toml                 workspace + the `kairo` CLI package
src/                       CLI adapter (main.rs, ui.rs)
crates/kairo-core/         domain + services, no UI dependencies
desktop/                   Vite + React + TypeScript front end
desktop/src-tauri/         Tauri v2 shell and command adapter
docs/                      this file, release notes, wiki sources
```

The desktop crate is a workspace member but not a default member, so
`cargo build`, `cargo test` and the existing release workflow still build only
the CLI and the core, with no GUI system libraries required.

## 4. Core design decisions

**Connection.** `db::Connection` wraps either an SQLite or a PostgreSQL pool and
exposes `list_tables`, `describe_table`, `fetch_rows`, `run_sql`,
`execute_batch`. It is opened from a `ConnectionTarget`, which is parsed once
and carries a password-free display form and a stable workspace key.

**Values.** Cells are `Value { kind, text, truncated }`. Integers travel as
text so 64-bit values survive JSON. Long text is clipped and blobs are
summarised, so one large row cannot stall the UI.

**PostgreSQL types.** User queries run through the simple query protocol, which
returns every value in text form. That makes `uuid`, `timestamptz`, `numeric`,
`jsonb`, arrays and enums displayable without a decoder per type. Table
browsing uses bound parameters and casts each column to `text` in SQL.

**Safety.** `services::safety::analyze` classifies SQL lexically as `read`,
`write` or `destructive`. The desktop command refuses to run anything above
`read` unless the caller passes a matching acknowledgement, so confirmation is
enforced in Rust and not only in the dialog. Unrecognised statements are
treated as destructive.

**Schema apply.** `plan` compares the parsed schema with the live database and
reports, per table, whether it will be created or already exists and how it
differs. `apply` runs the generated statements in one transaction. Kairo does
not alter existing tables; the plan says so explicitly.

**Grammar.** Extended, backward compatible: `[required]`, `[primary]`,
`[unique]` modifiers, float defaults, quoted identifiers, and the types
`float`, `blob`, `timestamp`. Every 0.4 schema parses unchanged and generates
the same SQL.

## 5. Desktop design decisions

- Passwords live only in Rust process memory for the life of a session. Recent
  connections store host, port, database and user. Reconnecting asks for the
  password again. No keychain integration in 1.0.
- Recents, settings and query history are one JSON file in the OS config
  directory, written atomically.
- File access goes through two narrow commands that only accept `.kairo`
  paths chosen in a native dialog, instead of a general filesystem plugin.
- No router, state library, component kit or editor library. The code editor is
  a textarea over a highlighted layer; highlighting is cosmetic and validation
  always comes from the Rust parser.
- Visual language follows kairo.arabdev.site: square corners, 1px hairline
  grids, the same graphite and accent tokens, Adelle for display type.

## 6. Phases

1. Audit and this document.
2. Extract `kairo-core`; move the CLI onto it; fix the audit findings.
3. Scaffold Tauri + Vite and get an empty window building.
4. Connection model, SQLite open and inspect.
5. Explorer: table list, structure, indexes, SQL, paginated data.
6. Query workspace with safety gate and history.
7. Schema workspace: edit, validate, preview, apply.
8. PostgreSQL through the same commands.
9. Export, project status, recents.
10. States, keyboard, accessibility pass.
11. README, changelog, release notes, screenshots.
12. Format, lint, test, production builds.

## 7. Known limits carried into 1.0

- One PostgreSQL schema per connection (`current_schema()`).
- Statement classification is lexical. A `SELECT` that calls a function with
  side effects is classified as a read.
- Schema apply creates tables. It does not migrate existing ones.
