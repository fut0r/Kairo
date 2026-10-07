# Welcome to the KairoDB Wiki

KairoDB reads, writes and manages databases using plain text. It is a command line tool and, from 1.0, a desktop app.

## v1.0.0

The first stable release.

- A desktop app for Windows, macOS and Linux: explore tables and rows, run queries with a safety check, edit and apply `.kairo` schemas, export a database's structure.
- The schema language gains `[primary]`, `[required]` and `[unique]`, the types `float`, `blob` and `timestamp`, decimal defaults and quoted names.
- Numbers are no longer printed as `null`, passwords are masked everywhere, and exported schemas can be applied again.
- The CLI and the app share one Rust core, `kairo-core`.

See the [changelog](https://github.com/fut0r/Kairo/blob/master/CHANGELOG.md) for everything.

## v0.4.0

Native multi-database support (PostgreSQL and SQLite), clear errors when a file is not a database, and standalone installation without a local Rust toolchain.

## v0.3.2

Commands like `read` and `export` validate file paths before connecting and return clear messages when something goes wrong.

## v0.3.1

Read and export any existing database file in human-readable form.

## Pages

- [Getting Started](Getting-Started)
- [Desktop App](Desktop-App)
- [Commands](Commands)
- [Schema Syntax](Schemas)
- [Query Language](Queries)
- [Architecture](Architecture)
- [Philosophy](Philosophy)
