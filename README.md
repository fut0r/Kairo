# KairoDB

Human-readable databases. Minimal. Fast. Local-first.

## What is it

KairoDB is a terminal-first database workflow for developers who want clarity over ceremony. Define schemas in plain text, validate them before they are applied, generate SQL in a predictable way, and inspect SQLite or PostgreSQL databases without opening a heavy GUI. The goal is simple: make the database layer feel readable, deliberate, and approachable.

## Install

### Download (no Rust required)

Download the binary for your OS from the [Releases](https://github.com/fut0r/kairo/releases) page.

After downloading, use the included installer script to set it up globally:

**Windows (PowerShell)**
```
.\install.ps1
```

**macOS / Linux**
```
chmod +x install.sh
./install.sh
```

Both scripts will place `kairo` in your PATH so you can run it from any directory.

### Build from source

```
git clone https://github.com/fut0r/kairo.git
cd kairo
cargo build --release
```

Then run the installer script above, or manually copy `target/release/kairo` to a directory in your PATH.

## Usage

```
kairo init               set up a new project
kairo create <name>      apply a .kairo schema to your database
kairo validate <name>    validate a schema before applying it
kairo query <sql>        run a query (supports natural syntax)
kairo read <file.db>     inspect any SQLite database file
kairo read <pg_url>      inspect a PostgreSQL database
kairo export <file.db>   convert a database to .kairo schema format
kairo tables             list all tables
kairo status             show project info
```

## Read any database

SQLite:
```
$ kairo read myapp.db

Database (SQLite): myapp.db
----------------------------------------
Tables: users, posts

table users {
  id: int
  name: string
  email: string
}

  -- 42 rows
  | id: 1 | name: alice | email: alice@example.com
  | id: 2 | name: bob | email: bob@example.com
  ...
```

PostgreSQL:
```
$ kairo read postgres://user:pass@localhost/mydb

Database (PostgreSQL): postgres://user:pass@localhost/mydb
----------------------------------------
Tables: users, orders

table users {
  id: int [required]
  name: string [required]
  email: string
}

  -- 1200 rows
  | id: 1 | name: alice | email: alice@example.com
  ...
```

## Schema syntax

```
table users {
  name: string
  age: int
  active: bool = true
}
```

Save as `schema/users.kairo`, then run `kairo create users`.

## Website

https://kairo.infiniware.bid

## License

GNU GPL 3.0

Made by fut0r (Zyad Mohamed)
