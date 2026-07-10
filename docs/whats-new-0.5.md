# What's New in KairoDB v0.5.0

KairoDB 0.5.0 is a focused release that strengthens the core value of the project: making databases feel readable, approachable, and deliberate.

## Highlights

- Added schema validation so developers can check a `.kairo` file before applying it.
- Improved parser support for practical field modifiers such as `required`, `primary`, and `unique`.
- Generated SQL now reflects those modifiers more faithfully.
- Natural query handling has been expanded to make the CLI easier to use in everyday work.
- The project scaffold now includes starter folders and a sample schema to reduce the blank-page feeling for new users.

## Why this release matters

The original idea behind KairoDB is not just to generate SQL. It is to make the database layer understandable without forcing a developer through needless ceremony. This release brings that idea closer to the surface by making the workflow feel less brittle and more deliberate.

## Upgrade notes

Existing projects remain compatible. The main change is that schemas can now express clearer structure, and the new `validate` command gives users a safer workflow before applying anything.
