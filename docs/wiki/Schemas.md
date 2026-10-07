# Kairo Schema Syntax

KairoDB uses a human-readable syntax to define database tables. Files are saved with the `.kairo` extension, normally in a project's `schema/` directory.

## Basic Table
```kairo
table <table_name> {
  <field_name>: <type>
}
```

## Supported Types

| Type | SQLite | PostgreSQL |
| --- | --- | --- |
| `string` | `TEXT` | `TEXT` |
| `int` | `INTEGER` | `INTEGER` |
| `float` | `REAL` | `DOUBLE PRECISION` |
| `bool` | `BOOLEAN` | `BOOLEAN` |
| `blob` | `BLOB` | `BYTEA` |
| `timestamp` | `TIMESTAMP` | `TIMESTAMP` |

A type that is not in this list is stored as text, and the validator warns about it.

## Default Values
Give a default with `=`. It can be `true`, `false`, a whole or decimal number, or text in double quotes.
```kairo
table posts {
  title: string = "untitled"
  published: bool = false
  views: int = 0
  rating: float = 4.5
}
```

## Modifiers
Modifiers go after the type and the default, in one pair of brackets.
```kairo
table users {
  id: int [primary]
  email: string [required, unique]
  name: string = "guest" [required]
}
```

| Modifier | Meaning |
| --- | --- |
| `primary` | Part of the primary key. Several `primary` fields make a composite key. |
| `required` | `NOT NULL` |
| `unique` | `UNIQUE` |

On SQLite, `id: int [primary]` is the row id and is assigned automatically. On PostgreSQL it is a plain integer key and you supply the values.

## Names
Names use letters, digits and `_`, and do not start with a digit. Anything else goes in double quotes:
```kairo
table "Order Items" {
  "unit price": float
}
```
Names that are SQL reserved words, such as `order` or `user`, need no quotes; Kairo quotes them in the SQL it generates.

## Formatting
- Fields can be separated by newlines or commas.
- Semicolons are not required.
- Comments start with `//`.

## Applying a schema
`kairo create <name>` and the desktop app's *Apply schema* create the tables that do not exist yet, in one transaction. Tables that already exist are not altered. Both report which tables were left unchanged and how they differ from the schema.

## Exporting
`kairo export <database>` writes a database's structure in this syntax. The output can be applied again. Where a column uses something the syntax cannot express, the export keeps it as a comment:
```kairo
table users {
  email: string [unique] // native: varchar(255)
  created_at: string // default: CURRENT_TIMESTAMP
}
```
