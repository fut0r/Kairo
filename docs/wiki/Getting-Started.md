# Getting Started with KairoDB

KairoDB is designed to be simple and local-first. Here is how you can get up and running in minutes.

## 1. Installation

### Without Rust (Pre-built Binaries)
1. Go to the [Releases](https://github.com/fut0r/Kairo/releases) page.
2. Download the binary for your operating system (Linux, macOS, or Windows).
3. Run `install.sh` or `install.ps1` beside it, or move the binary to a directory in your system's PATH.

The same page has installers for the [desktop app](Desktop-App).

### With Rust (From Source)
If you have Rust installed:
```bash
cargo install --path .
```

## 2. Initialize your project
Run the following command in your project directory:
```bash
kairo init
```
This creates the following structure:
- `kairo.config`: Main configuration file.
- `schema/`: Where you define your tables.
- `data/`: Local storage (for SQLite).
- `migrations/`, `queries/`, `plugins/`: Expansion folders.

## 3. Define a schema
Create a file `schema/users.kairo`:
```kairo
table users {
  name: string
  age: int
  active: bool = true
}
```

## 4. Apply the schema
```bash
kairo create users
```

## 5. Query your data
```bash
kairo query "from users"
```
