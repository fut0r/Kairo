# Changelog

All notable changes to KairoDB are recorded here. Versions follow [Semantic Versioning](https://semver.org).

## 1.0.0

The first stable release. It adds a desktop app and moves everything Kairo does into one Rust core that the app and the CLI share.

### Added

- **Desktop app** for Windows, macOS and Linux, built with Tauri v2, React and TypeScript.
  - Overview: tables, views, row counts, file size, and the status of a project folder.
  - Explorer: expandable table list; data with paging, filtering and sorting; structure; indexes and foreign keys; SQL.
  - Query: SQL editor, typed results with timing, per-database history, confirmation before anything that changes data.
  - Schema: `.kairo` editor with inline validation from the real parser, a preview of the parsed schema, and an apply dialog that shows the plan and the target first.
  - Export: a database's structure as `.kairo`.
  - Settings: dark and light themes, page size, row cap, query time limit.
- **Schema language**
  - Modifiers: `[primary]`, `[required]`, `[unique]`.
  - Types: `float`, `blob`, `timestamp`.
  - Decimal defaults, such as `price: float = 9.99`.
  - Quoted names for tables and fields, such as `table "Order Items"`.
  - Diagnostics with line and column, and plain explanations of what to change.
- `kairo create` reports tables that already exist and how they differ from the schema.
- `kairo query` reports rows affected for statements that change data.
- TLS for PostgreSQL connections.
- `kairo-core`, a library crate holding the parser, both database adapters and every use case.

### Changed

- Version 1.0.0 across the CLI, the core, the desktop app and the website.
- sqlx 0.7 to 0.9. The minimum Rust version is now 1.94.
- `kairo read` and `kairo export` open SQLite files read-only.
- `kairo export` writes only what the parser accepts. Anything it cannot express, such as a `CURRENT_TIMESTAMP` default or a native type like `varchar(255)`, is kept as a trailing comment.
- Text defaults are generated as SQL string literals (`DEFAULT 'text'`).
- The project's links point to `github.com/fut0r/Kairo` and `kairo.arabdev.site`.

### Fixed

- Integer, real and blob values were printed as `null` by `kairo query` and `kairo read`.
- `kairo tables` only worked on SQLite.
- Schemas with more than one table failed to apply on PostgreSQL.
- Text defaults failed on PostgreSQL.
- SQLite files whose path contains a space, `?` or `#` could not be opened.
- Tables whose names are reserved words or contain unusual characters could not be read.
- Files written by `kairo export` could not be applied with `kairo create`.
- Row counts above 2,147,483,647 overflowed.

### Security

- `kairo read` and `kairo export` printed PostgreSQL passwords. Passwords are now masked in everything Kairo displays, logs, stores or exports.
- Table and column names are quoted or bound in every statement Kairo builds.

### Compatibility

- Every 0.4 command, argument and `kairo.config` works as before.
- Every 0.4 schema parses unchanged and generates the same SQL.
- Release assets for the CLI keep their names: `kairo-linux`, `kairo-macos`, `kairo-windows.exe`.

## 0.4.0

- `read` and `export` work with PostgreSQL connection strings as well as SQLite files.
- Clear error messages when a file is not a database.
- Standalone install scripts, `install.sh` and `install.ps1`.

## 0.3.2

- `read` and `export` validate the path before connecting and report problems plainly.

## 0.3.1

- `read` and `export`: inspect any database file and write it as a `.kairo` schema.
