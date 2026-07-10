# Kairo Schema Syntax

KairoDB uses a human-readable syntax to define database tables. Files should be saved with the `.kairo` extension in the `schema/` directory.

## Basic Table
```kairo
table <table_name> {
  <field_name>: <type>
}
```

## Supported Types
- `string`: Maps to `TEXT` (SQLite) or `VARCHAR/TEXT` (PostgreSQL).
- `int`: Maps to `INTEGER`.
- `bool`: Maps to `BOOLEAN`.

## Modifiers and Defaults
You can emphasize structure directly in the schema with lightweight modifiers and defaults.
```kairo
table posts {
  id: int [primary]
  title: string [required]
  published: bool [required] = false
  views: int = 0
  slug: string [unique]
}
```

## Formatting
- Fields can be separated by newlines or commas.
- Semicolons are not required.
- Comments can be added using `//`.
- The validator command can be used to check schemas before applying them.
