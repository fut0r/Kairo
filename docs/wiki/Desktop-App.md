# The Desktop App

The desktop app sits on the same Rust core as the `kairo` command. It shows the SQL it runs and, where there is one, the equivalent command.

## Opening a database

- **SQLite**: click *Open Database* and pick a file. *New Connection* lets you type a path, test it, or create a new file.
- **PostgreSQL**: click *New Connection* and paste a URL such as `postgres://user@localhost:5432/database`. *Test connection* tries it without saving anything.
- **Project**: on the Overview, *Open a project folder* reads `kairo.config` and connects to the database it names.

Everything you open is listed under *Recent* in the sidebar.

## Pages

| Page | Shortcut | What it is for |
| --- | --- | --- |
| Overview | Ctrl+1 | Tables, views, row counts, file size, project status |
| Explorer | Ctrl+2 | Browse a table: Data, Structure, Indexes, SQL |
| Query | Ctrl+3 | Write and run SQL |
| Schema | Ctrl+4 | Edit, validate, preview and apply a `.kairo` file |
| Export | Ctrl+5 | The database's structure as `.kairo` |
| Settings | Ctrl+6 | Theme, limits, privacy |

On macOS the shortcuts use ⌘ instead of Ctrl.

## Explorer

- Click the arrow beside a table to list its columns and types.
- **Data**: type in *Filter rows* to search every column. Click a column header to sort; click again to reverse; a third time clears it. Choose the page size on the right.
- Click a cell, or use the arrow keys, to see its full value under the grid. Ctrl+C copies it.
- The statement behind the page is shown under the grid.

## Query

- Ctrl+Enter runs the query.
- A query that starts with `from` gets `SELECT *` put in front of it: `from users where age > 18`.
- Reads run at once. Statements that change data ask first and name the database. `DROP`, `DELETE`, `UPDATE`, `ALTER` and `TRUNCATE` always ask.
- Results are capped at 1,000 rows and statements stop after 30 seconds. Both can be changed in Settings.
- History is kept per database. Click an entry to bring it back.

## Schema

- The editor checks the file with Kairo's own parser as you type. Problems are marked on the line and listed under the editor; click one to jump to it.
- *Preview* shows each table and field as parsed, with the column type it becomes.
- The SQL tab shows what applying will run, in the dialect of the open database.
- *Apply schema…* compares the schema with the database and shows the plan. Nothing changes until you confirm.

Applying creates tables that do not exist. Tables that already exist are left exactly as they are, and the plan lists how they differ.

## What Kairo remembers

Settings, recent connections and query history, in one file on your machine. **Passwords are never saved.** A saved PostgreSQL connection keeps the host, port, database and user, and asks for the password each time. *Settings → Privacy* clears recents and history.
