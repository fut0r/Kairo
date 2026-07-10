# Welcome to the KairoDB Wiki

KairoDB is a terminal-first database workflow for people who want clarity over ceremony. It reads, writes, and inspects databases using plain text and a calm, predictable CLI.

## v0.5.0

This release focuses on reliability and clarity. It introduces schema validation, better modifier support for required/primary/unique fields, smarter natural query handling, and a more polished project layout so new users can start quickly without feeling like they are fighting the tool.

See the [What's New in v0.5.0](../whats-new-0.5.md) guide for the full release summary.

## v0.4.0

This release introduced native multi-database support (PostgreSQL and SQLite), elegant format mismatch error logging, and standalone installation support so developers could run KairoDB globally without requiring a local Rust environment.

## v0.3.2

This version improves error handling across the CLI. Commands like `read` and `export` now validate file paths before attempting a database connection, and return clear, human-readable messages when something goes wrong. No more raw stack traces.

## v0.3.1

This version adds the ability to read and export any existing database file into human-readable format.

## Pages

- [Getting Started](Getting-Started)
- [Commands](Commands)
- [Schema Syntax](Schemas)
- [Query Language](Queries)
- [Architecture](Architecture)
- [Philosophy](Philosophy)
