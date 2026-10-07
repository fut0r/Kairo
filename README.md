# KairoDB

Human-readable databases. Minimal. Fast. Local-first.

KairoDB lets you define database schemas in plain `.kairo` files, apply them to SQLite or PostgreSQL, inspect any existing database, run queries, and export a database's structure back to `.kairo`. It comes as a command line tool and, from 1.0, a desktop app built on the same Rust core.

No ORM, no account, no telemetry. Nothing leaves your machine.

![The Explorer showing rows of a table](docs/screenshots/02-explorer.png)

## Contents

- [Install](#install)
- [The desktop app](#the-desktop-app)
- [The command line](#the-command-line)
- [Schema syntax](#schema-syntax)
- [Safety](#safety)
- [Privacy and credentials](#privacy-and-credentials)
- [PostgreSQL](#postgresql)
- [Build from source](#build-from-source)
- [Tests](#tests)
- [Project layout](#project-layout)
- [License](#license)

## Install

Download from the [Releases](https://github.com/fut0r/Kairo/releases) page. No Rust required.

**Desktop app**

| Platform | File |
| --- | --- |
| Windows | the `-setup.exe` installer, or the `.msi` |
| macOS | the `aarch64.dmg` for Apple silicon, or the `x64.dmg` for Intel |
| Linux | the `.AppImage`, `.deb` or `.rpm` |

The installers are built by the release workflow and are not code-signed. Windows SmartScreen and macOS Gatekeeper will ask you to confirm the first launch.

**Command line**

Download `kairo-windows.exe`, `kairo-macos` or `kairo-linux`, then run the installer script next to it to put `kairo` on your PATH:

```
.\install.ps1          # Windows (PowerShell)
```

```
chmod +x install.sh    # macOS / Linux
./install.sh
```

## The desktop app

The app makes the main workflows visible. It does not replace the terminal: every screen shows the SQL it runs, and where there is an equivalent `kairo` command it shows that too.

| | |
| --- | --- |
| **Overview** | Tables, views, row counts and file size for the open database, plus the status of a project folder. |
| **Explorer** | An expandable table list. For each table: paged data with filter and sort, structure, indexes and foreign keys, and its SQL. |
| **Query** | An SQL editor. Reads run at once; anything that changes data asks first. Results are typed, timed and capped. History is kept per database. |
| **Schema** | Edit a `.kairo` file with live validation from the real parser, preview what it parses to, and apply it. |
| **Export** | The open database's structure as `.kairo`, to copy, save, or open in the Schema workspace. |
| **Settings** | Theme, page size, row cap, query time limit, and what Kairo remembers. |

![Running a query](docs/screenshots/03-query.png)

Editing a schema, with the parsed preview beside it:

![The Schema workspace](docs/screenshots/05-schema.png)

Applying shows the target database and exactly what will happen before anything changes:

![The apply dialog](docs/screenshots/07-apply-plan.png)

Light theme:

![The Explorer in the light theme](docs/screenshots/12-light.png)

### Opening a database

- **SQLite**: *Open Database* in the sidebar opens the native file picker. *New Connection* also lets you type a path, test it first, or create a new file.
- **PostgreSQL**: *New Connection*, then paste a URL such as `postgres://user@localhost:5432/database`. Put the password in the password field or in the URL. *Test connection* tries it without saving anything.
- **Project**: on the Overview, *Open a project folder* reads its `kairo.config` and connects to the configured database.

### Keyboard

| Keys | Action |
| --- | --- |
| `Ctrl`/`⌘` + `1`…`6` | Overview, Explorer, Query, Schema, Export, Settings |
| `Ctrl`/`⌘` + `O` | Open a database |
| `Ctrl`/`⌘` + `Enter` | Run the query |
| `Ctrl`/`⌘` + `S` | Save the schema |
| `Esc`, then `Tab` | Leave a code editor (`Tab` alone indents) |
| Arrow keys, `Home`, `End`, `PageUp`, `PageDown` | Move between cells in a result grid |
| `Ctrl`/`⌘` + `C` | Copy the active cell |

## The command line

```
kairo init               set up a new project in the current folder
kairo create <name>      apply schema/<name>.kairo to the project database
kairo query <sql>        run a query against the project database
kairo read <target>      describe a database: tables, columns, first rows
kairo export <target>    write a database's structure as .kairo
kairo tables             list the project database's tables
kairo status             show the project's configuration
```

`<target>` is a SQLite file or a PostgreSQL URL. `kairodb` is the same program under its longer name.

```
$ kairo init
  OK initialized.

$ kairo create users
  sql:
CREATE TABLE IF NOT EXISTS users (
  name TEXT,
  age INTEGER
);

  OK applied schema 'users' to data/kairo.db

$ kairo query "from users where age > 18"
  -> SELECT * from users where age > 18
name | age
-------------
alice | 30
bob | 41
  2 rows

$ kairo export data/kairo.db -o schema/exported.kairo
  OK exported schema to schema/exported.kairo
```

`kairo query` accepts a short form: a query that starts with `from` gets `SELECT *` put in front of it.

A project is a folder with a `kairo.config`:

```toml
adapter = "sqlite"            # or "postgres"
database = "data/kairo.db"    # or a postgres:// URL
```

The CLI runs what you give it without asking for confirmation. The confirmation described under [Safety](#safety) belongs to the desktop app.

## Schema syntax

```
// Comments start with two slashes.
table users {
  id: int [primary]
  name: string [required]
  email: string [required, unique]
  active: bool = true
  score: float = 0.0
  joined_at: timestamp
}
```

| Type | SQLite | PostgreSQL |
| --- | --- | --- |
| `string` | `TEXT` | `TEXT` |
| `int` | `INTEGER` | `INTEGER` |
| `float` | `REAL` | `DOUBLE PRECISION` |
| `bool` | `BOOLEAN` | `BOOLEAN` |
| `blob` | `BLOB` | `BYTEA` |
| `timestamp` | `TIMESTAMP` | `TIMESTAMP` |

- **Modifiers** go in one pair of brackets: `[primary]`, `[required]` (NOT NULL), `[unique]`. Marking several fields `[primary]` makes a composite key.
- **Defaults** are `true`, `false`, a number, or `"quoted text"`.
- **Names** use letters, digits and `_`. Anything else goes in quotes: `table "Order Items" { "unit price": float }`.
- Fields are separated by new lines or commas.
- An unknown type is stored as text, with a warning.

Applying a schema creates the tables that do not exist. **It does not alter tables that already exist**: Kairo creates, it does not migrate. The desktop app's apply dialog and `kairo create` both say which tables were left unchanged and how they differ from the schema.

Every schema that worked in 0.4 works unchanged and generates the same SQL.

## Safety

The desktop app classifies SQL before running it:

| Risk | Examples | What happens |
| --- | --- | --- |
| read | `SELECT`, `EXPLAIN`, `PRAGMA table_info(...)` | Runs at once |
| write | `INSERT`, `CREATE`, `VACUUM` | Asks first. Can be turned off in Settings |
| destructive | `DROP`, `DELETE`, `UPDATE`, `ALTER`, `TRUNCATE`, `INSERT OR REPLACE` | Always asks |

![Confirming a script that changes rows](docs/screenshots/04-confirm.png)

- The dialog names the database, lists the reasons, and puts focus on *Cancel*.
- The check is enforced in the Rust backend, not only in the dialog: `run_query` refuses anything above a read unless the request carries a matching acknowledgement.
- Results are capped (1,000 rows by default) and statements are stopped after a time limit (30 seconds by default). Both are in Settings.
- Row filters are bound parameters. Table and column names are checked against the catalog and quoted.

Limits worth knowing:

- The classification reads keywords. It cannot see inside a function, so `SELECT some_function()` counts as a read whatever the function does.
- A statement Kairo does not recognise is gated like a destructive one, because it cannot be ruled out. This includes typos such as `SELEC`.
- A script with several statements runs statement by statement, as it would in any SQL client. Wrap it in `BEGIN … COMMIT` if you need all or nothing. Applying a schema is different: that always runs in one transaction.

## Privacy and credentials

- No account, no telemetry, no network requests other than the database connections you make.
- **Passwords are never written to disk.** A PostgreSQL password is held in memory for the session. A saved connection keeps the host, port, database and user, and asks for the password again next time. There is no keychain integration in 1.0.
- Passwords are masked wherever a connection is shown: the header, dialogs, error messages, the activity log, exported files and `kairo read` output.
- SQL is redacted before it enters query history: connection URLs, `password=…` settings and `PASSWORD '…'` literals.
- Settings, recent connections and history live in one JSON file:

  | Platform | Location |
  | --- | --- |
  | Windows | `%APPDATA%\site.arabdev.kairo\kairo.json` |
  | macOS | `~/Library/Application Support/site.arabdev.kairo/kairo.json` |
  | Linux | `~/.config/site.arabdev.kairo/kairo.json` |

  Set `KAIRO_CONFIG_DIR` to keep it somewhere else. *Settings → Privacy* clears recents and history.

## PostgreSQL

- Connect with a `postgres://` or `postgresql://` URL. TLS is supported; add `?sslmode=require` where the server needs it.
- Kairo works in the connection's current schema, normally `public`. To use another, add `?options=-c%20search_path%3Dmyschema` to the URL.
- Types Kairo has no special handling for (`uuid`, `jsonb`, arrays, `numeric`, enums, ranges) are shown as the server prints them.
- Row counts in the table list are the planner's estimates, marked `~`. Opening a table counts exactly.
- Queries are sent without named prepared statements, so connections through PgBouncer and hosted poolers work.
- `KAIRO_PG_POOL_SIZE` (default 3) sets how many connections Kairo keeps per database. Use `1` for servers that put every client on one session, such as PGlite.

### How PostgreSQL was verified

The PostgreSQL adapter has an end-to-end test, `crates/kairo-core/tests/postgres.rs`, and the desktop app has a matching scenario, `desktop/e2e/postgres.mjs`. For 1.0 both were run against [PGlite](https://pglite.dev), which is the PostgreSQL engine compiled to WebAssembly, and both pass. The CI workflow is set up to run the Rust test against a `postgres:16` container on every push.

Not verified against a live server before release, because PGlite cannot do them:

- **The statement time limit on PostgreSQL.** It relies on the server's `statement_timeout`. The test for it exists and is part of the CI job.
- **TLS connections.**
- **Several connections at once.** PGlite serves all clients from one session.

If you hit a problem with a real server, please open an issue with the server version.

## Build from source

You need [Rust](https://rustup.rs) 1.94 or newer. For the desktop app you also need [Node.js](https://nodejs.org) 22 or newer and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```
git clone https://github.com/fut0r/Kairo.git
cd Kairo
```

**CLI**

```
cargo build --release
./target/release/kairo --version
```

**Desktop app**

```
cd desktop
npm install
npm run tauri dev        # run with live reload
npm run tauri build      # build installers into target/release/bundle
```

`npm run tauri build -- --no-bundle` builds only the executable, `target/release/kairo-desktop`.

`cargo build` and `cargo test` at the repository root cover the CLI and the core only, so they need no GUI libraries.

### Fonts

The app uses the same typefaces as [kairo.arabdev.site](https://kairo.arabdev.site): Adelle and Helvetica Neue, loaded from `Adelle-Font/` and `helvetica-neue-5/` at the repository root, and JetBrains Mono (SIL Open Font License) from npm. **Adelle and Helvetica Neue are commercial typefaces.** Check that your licences cover embedding them in a distributed application before you publish installers. To build without them, delete the `import "./styles/fonts.css"` line in `desktop/src/main.tsx`; the interface falls back to system fonts.

## Tests

```
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings      # CLI and core
cargo test                                     # CLI and core

cd desktop
npm install
npm run lint
npm test                                       # frontend unit and component tests
npm run build                                  # type-check and production bundle
cd ..

cargo clippy --workspace --all-targets -- -D warnings   # adds the desktop crate
cargo test --workspace
```

The `--workspace` forms include the desktop crate, which embeds the built frontend at compile time. Run `npm run build` in `desktop/` before them.

PostgreSQL tests run when a server is given:

```
KAIRO_TEST_POSTGRES_URL=postgres://postgres:secret@localhost:5432/postgres \
  cargo test -p kairo-core --test postgres
```

On Windows, `npm run e2e` (in `desktop/`) starts the built app and drives the real window through WebView2's debugging port: it opens a real SQLite file, applies a schema, runs queries, pages through rows and checks 69 behaviours, with nothing mocked. See `desktop/e2e/run-e2e.ps1`.

## Project layout

```
Cargo.toml             workspace, and the kairo CLI package
src/                   CLI: argument parsing and terminal output
crates/kairo-core/     schema parser, SQLite and PostgreSQL adapters, services
desktop/               desktop app: React + TypeScript + Vite
desktop/src-tauri/     Tauri v2 shell and command adapter
desktop/e2e/           end-to-end scenarios for the built app
docs/                  architecture, release notes, wiki sources
index.html, style.css  the website
```

The CLI and the desktop app call the same functions in `kairo-core`. The design is described in [docs/v1-architecture.md](docs/v1-architecture.md).

## License

GNU GPL 3.0. See [LICENSE](LICENSE). KairoDB is free software and comes with no warranty.

Website: https://kairo.arabdev.site

Made by fut0r (Zyad Mohamed)
