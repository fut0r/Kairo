# KairoDB Commands

## `init`
Set up a new KairoDB project in the current directory.
```
kairo init
```
Creates: `schema/`, `data/`, `migrations/`, `queries/`, `plugins/`, and `kairo.config`.

## `create <name>`
Reads `schema/<name>.kairo`, generates SQL, and applies it to the configured database.
```
kairo create users
```

## `validate <name>`
Checks a schema file for correctness before it is applied.
```
kairo validate users
```

## `query <sql>`
Runs a query against the database. Supports natural syntax and raw SQL.
```
kairo query "from users"
kairo query "SELECT * FROM users WHERE age > 18"
```

## `read <file>`
Reads any SQLite database file and prints a human-readable breakdown of its structure and data.
```
kairo read myapp.db
```

## `export <file>`
Converts a database file into .kairo schema format. Optionally writes to a file.
```
kairo export myapp.db
kairo export myapp.db -o schema/imported.kairo
```

## `tables`
Lists all tables in the current project database.
```
kairo tables
```

## `status`
Shows current project configuration and stats.
```
kairo status
```
