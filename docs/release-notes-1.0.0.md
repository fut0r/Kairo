## KairoDB 1.0.0

The first stable release. Kairo now has a desktop app, and the app and the command line run on the same Rust core.

### A desktop app

Open a SQLite file or connect to PostgreSQL, then:

- **Explore**: tables, columns, indexes, foreign keys, and rows with paging, filtering and sorting.
- **Query**: write SQL, run it with Ctrl+Enter, get typed results with timing. History is kept per database.
- **Edit schemas**: write `.kairo` with live validation from the real parser, preview what it means, and apply it.
- **Export**: any database's structure as a `.kairo` file.

It is local-first. No account, no telemetry. Passwords are never written to disk.

### Built to be safe

- Reads run at once. Anything that changes data asks first, and names the database it will change.
- `DROP`, `DELETE`, `UPDATE`, `ALTER` and `TRUNCATE` always ask. The check is enforced in the backend, not only in the dialog.
- Applying a schema shows the plan before it does anything, runs in one transaction, and never alters a table that already exists.
- Results are capped and long statements are stopped.

### A bigger schema language

```
table users {
  id: int [primary]
  email: string [required, unique]
  score: float = 0.0
  joined_at: timestamp
}
```

New: `[primary]`, `[required]`, `[unique]`; the types `float`, `blob` and `timestamp`; decimal defaults; quoted names. Errors now point at the line and column and say what to change.

Every schema from 0.4 works unchanged.

### Fixes worth upgrading for

- `kairo query` and `kairo read` printed `null` for every number. They print the number.
- `kairo read` and `kairo export` printed PostgreSQL passwords. They are masked now, everywhere.
- `kairo export` wrote files that `kairo create` could not read. Exports now round-trip.
- `kairo tables` and multi-table schemas work on PostgreSQL.
- PostgreSQL connections can use TLS.
- SQLite paths with spaces work.

### Install

**Desktop app**: download the installer for your platform below. The installers are not code-signed, so Windows and macOS will ask you to confirm the first launch.

**Command line**: download `kairo-windows.exe`, `kairo-macos` or `kairo-linux`, then run `install.ps1` or `install.sh` beside it.

### Known limits

- Kairo creates tables. It does not migrate existing ones.
- SQL is classified by its keywords. A `SELECT` that calls a function with side effects counts as a read.
- PostgreSQL: one schema per connection. The statement time limit, TLS and concurrent connections were not exercised against a live server before this release; see the README.
- No system keychain integration. A saved PostgreSQL connection asks for its password each time.

Full list of changes: [CHANGELOG.md](https://github.com/fut0r/Kairo/blob/master/CHANGELOG.md)
