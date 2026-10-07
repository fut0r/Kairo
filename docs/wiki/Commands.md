# KairoDB Commands

`kairo` and `kairodb` are the same program.

## `init`
Set up a new KairoDB project in the current directory.
```
kairo init
```
Creates: `schema/`, `data/`, `migrations/`, `queries/`, `plugins/`, and `kairo.config`. Files that already exist are kept.

## `create <name>`
Reads `schema/<name>.kairo`, prints the SQL it generates, and applies it to the configured database.
```
kairo create users
```
Tables are created in one transaction. A table that already exists is left as it is, and `create` says so and lists how it differs from the schema.

## `query <sql>`
Runs a query against the project database. Supports the short form and raw SQL.
```
kairo query "from users"
kairo query "SELECT * FROM users WHERE age > 18"
```
For statements that change data it prints the number of rows affected.

## `read <target>`
Prints a human-readable breakdown of a database: its tables, their columns, the row count and the first five rows of each. `<target>` is a SQLite file or a PostgreSQL URL.
```
kairo read myapp.db
kairo read postgres://user:secret@localhost:5432/mydb
```
The file is opened read-only. A password in a URL is masked in the output.

## `export <target>`
Writes a database's structure in `.kairo` schema format. Prints to the terminal, or to a file with `-o`.
```
kairo export myapp.db
kairo export myapp.db -o schema/imported.kairo
```
The result can be applied with `kairo create`.

## `tables`
Lists the tables in the project database.
```
kairo tables
```

## `status`
Shows the project's configuration: adapter, database, number of schemas, and whether the database file exists.
```
kairo status
```

## Options
```
kairo --version
kairo --help
kairo <command> --help
```
