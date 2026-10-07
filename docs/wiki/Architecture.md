# Architecture

KairoDB is one Rust core with two front ends: the `kairo` command and the desktop app.

```
  CLI (src/)                 Desktop (desktop/)
  clap + terminal output     Tauri commands + React
            \                     /
             \                   /
              kairo-core (crates/kairo-core)
              ├─ services   query, safety, schema_apply, export, report, project
              ├─ schema     grammar, parser, SQL generation, rendering
              └─ db         connection targets, SQLite and PostgreSQL adapters, values
```

## Core components

### Schema (`kairo_core::schema`)
The grammar in `kairo.pest` is parsed with Pest into a `Schema`. `validate` returns diagnostics with line and column; `sqlgen` turns a schema into `CREATE TABLE` statements for a dialect; `render` writes a schema back out as `.kairo` text.

### Database (`kairo_core::db`)
`Connection` wraps either a SQLite or a PostgreSQL pool behind one interface: list tables, describe a table, fetch a page of rows, run SQL, run a batch in a transaction. Results are structured values, never formatted text.

### Services (`kairo_core::services`)
The use cases. Both front ends call the same functions:

- `query`: translate the short query form, analyse, run.
- `safety`: classify SQL as read, write or destructive.
- `schema_apply`: preview a schema, plan it against a database, apply it.
- `export`: read a database's structure into a schema.
- `report`: the text report printed by `kairo read`.
- `project`: `kairo.config`, `kairo init`, project status.

### CLI (`src/`)
Parses arguments with Clap, calls a service, prints the result.

### Desktop (`desktop/`)
`desktop/src-tauri` exposes the services as Tauri commands and keeps the open connections and the settings file. `desktop/src` is the React interface. It contains no schema parsing and no SQL generation.

## Rules

- The core never prints. Only `services::report` produces terminal text.
- Errors are one type with a `kind`, a message, an optional detail and an optional hint. Front ends decide how to show an error from its kind.
- Every string in an error is redacted first, so a password cannot leave through one.
- Table and column names are quoted or bound in every statement.

The full design notes are in [docs/v1-architecture.md](https://github.com/fut0r/Kairo/blob/master/docs/v1-architecture.md).
