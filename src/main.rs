mod ui;
mod core;
mod adapters;
mod config;

use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use crate::core::adapter::Adapter;
use crate::adapters::sqlite::SqliteAdapter;
use crate::adapters::postgres::PostgresAdapter;

#[derive(Parser)]
#[command(name = "kairo")]
#[command(version = "0.5.0")]
#[command(about = "Human-readable databases. Minimal. Fast. Local-first.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new KairoDB project
    Init,

    /// Create a table from a .kairo schema file
    Create {
        /// Name of the schema (matches schema/<name>.kairo)
        name: String,
    },

    /// Run a query against the database
    Query {
        /// SQL or natural query string
        query: String,
    },

    /// Read a database file and show its structure in human-readable format
    Read {
        /// Path to the database file
        file: String,
    },

    /// Export a database file to .kairo schema format
    Export {
        /// Path to the database file
        file: String,
        /// Output file (optional, prints to stdout if not given)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// List all tables in the current database
    Tables,

    /// Validate a .kairo schema without applying it
    Validate {
        /// Name of the schema (matches schema/<name>.kairo)
        name: String,
    },

    /// Show current project status
    Status,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run_cli(cli).await {
        let err_str = e.to_string();
        if err_str.contains("code: 26") || err_str.contains("file is not a database") {
            ui::print_error("the file is not a valid database format or is corrupted.");
        } else {
            ui::print_error(&err_str);
        }
        std::process::exit(1);
    }
}

async fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Commands::Init) => {
            init_project()?;
        }

        Some(Commands::Create { name }) => {
            let schema_path = std::path::PathBuf::from("schema").join(format!("{}.kairo", name));
            let content = std::fs::read_to_string(&schema_path)
                .map_err(|_| anyhow!("no schema file at {}", schema_path.display()))?;

            // Strip BOM
            let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);

            let schema = core::parser::parse_schema(content)?;
            let sql = core::schema::generate_sql(&schema);

            ui::print_dim("sql:");
            println!("{}", sql);

            let config = config::load_config().ok();
            if let Some(cfg) = config {
                let adapter = connect_adapter(&cfg).await?;
                adapter.execute(&sql).await?;
                ui::print_success(&format!("applied schema '{}' to {}", name, cfg.database));
            } else {
                ui::print_dim("no kairo.config found, schema not applied to any database");
            }
        }

        Some(Commands::Query { query }) => {
            let config = config::load_config()
                .map_err(|_| anyhow!("no kairo.config found. run 'kairo init' first."))?;

            let adapter = connect_adapter(&config).await?;

            let sql = core::query::translate_query(&query);
            if sql != query {
                ui::print_dim(&format!("-> {}", sql));
            }

            let rows = adapter.query(&sql).await?;
            if rows.is_empty() {
                ui::print_dim("(no rows)");
            } else {
                // Print header
                let headers: Vec<&str> = rows[0].columns.iter().map(|c| c.name.as_str()).collect();
                println!("{}", headers.join(" | "));
                println!("{}", "-".repeat(headers.iter().map(|h| h.len() + 3).sum::<usize>()));

                for row in &rows {
                    let fields: Vec<&str> = row.columns.iter().map(|c| c.value.as_str()).collect();
                    println!("{}", fields.join(" | "));
                }
                ui::print_dim(&format!("{} rows", rows.len()));
            }
        }

        Some(Commands::Read { file }) => {
            let is_postgres = file.starts_with("postgres://") || file.starts_with("postgresql://");
            if !is_postgres {
                let path = std::path::Path::new(&file);
                if path.extension().map_or(false, |ext| ext == "kairo") {
                    return Err(anyhow!(
                        "file '{}' is a .kairo schema file. 'kairo read' is used to inspect SQLite database files (e.g., 'data/kairo.db').",
                        file
                    ));
                }

                if !path.exists() {
                    let mut err_msg = format!("database file '{}' not found.", file);
                    if file == "kairo.db" && std::path::Path::new("data/kairo.db").exists() {
                        err_msg.push_str(" Did you mean 'data/kairo.db'?");
                    }
                    return Err(anyhow!(err_msg));
                } else if !path.is_file() {
                    return Err(anyhow!("'{}' is not a file.", file));
                }
            }

            let output = core::reader::read_database(&file).await?;
            print!("{}", output);
        }

        Some(Commands::Export { file, output }) => {
            let is_postgres = file.starts_with("postgres://") || file.starts_with("postgresql://");
            if !is_postgres {
                let path = std::path::Path::new(&file);
                if path.extension().map_or(false, |ext| ext == "kairo") {
                    return Err(anyhow!(
                        "file '{}' is a .kairo schema file. 'kairo export' is used to convert SQLite database files (e.g., 'data/kairo.db') to schema format.",
                        file
                    ));
                }

                if !path.exists() {
                    let mut err_msg = format!("database file '{}' not found.", file);
                    if file == "kairo.db" && std::path::Path::new("data/kairo.db").exists() {
                        err_msg.push_str(" Did you mean 'data/kairo.db'?");
                    }
                    return Err(anyhow!(err_msg));
                } else if !path.is_file() {
                    return Err(anyhow!("'{}' is not a file.", file));
                }
            }

            let schema = core::reader::export_schema(&file).await?;
            match output {
                Some(path) => {
                    std::fs::write(&path, &schema)?;
                    ui::print_success(&format!("exported schema to {}", path));
                }
                None => {
                    print!("{}", schema);
                }
            }
        }

        Some(Commands::Validate { name }) => {
            let schema_path = std::path::PathBuf::from("schema").join(format!("{}.kairo", name));
            let content = std::fs::read_to_string(&schema_path)
                .map_err(|_| anyhow!("no schema file at {}", schema_path.display()))?;
            let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
            let _ = core::parser::parse_schema(content)?;
            ui::print_success(&format!("schema '{}' is valid", name));
        }

        Some(Commands::Tables) => {
            let config = config::load_config()
                .map_err(|_| anyhow!("no kairo.config found. run 'kairo init' first."))?;
            let adapter = connect_adapter(&config).await?;
            let query = if config.adapter == "sqlite" {
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            } else {
                "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name"
            };
            let rows = adapter.query(query).await?;

            if rows.is_empty() {
                ui::print_dim("(no tables)");
            } else {
                ui::print_header("tables");
                for row in &rows {
                    let table_name = row.columns.first().map(|c| c.value.as_str()).unwrap_or("");
                    ui::print_row("-", table_name);
                }
            }
        }

        Some(Commands::Status) => {
            ui::print_header("kairo v0.5.0");

            match config::load_config() {
                Ok(cfg) => {
                    ui::print_row("adapter ", &cfg.adapter);
                    ui::print_row("database", &cfg.database);

                    let schema_count = std::fs::read_dir("schema")
                        .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| {
                            e.path().extension().map(|ext| ext == "kairo").unwrap_or(false)
                        }).count())
                        .unwrap_or(0);

                    ui::print_row("schemas ", &schema_count.to_string());

                    if cfg.adapter == "sqlite" {
                        let exists = std::path::Path::new(&cfg.database).exists();
                        ui::print_row("db file ", if exists { "exists" } else { "not created yet" });
                    }
                }
                Err(_) => {
                    ui::print_dim("not initialized. run 'kairo init'.");
                }
            }
        }

        None => {
            ui::print_welcome();
        }
    }

    Ok(())
}

async fn connect_adapter(cfg: &config::Config) -> Result<Box<dyn Adapter>> {
    if cfg.adapter == "sqlite" {
        let mut a = SqliteAdapter::new();
        let conn_str = if cfg.database.starts_with("sqlite:") {
            cfg.database.clone()
        } else {
            format!("sqlite:{}?mode=rwc", cfg.database)
        };
        a.connect(&conn_str).await?;
        Ok(Box::new(a))
    } else if cfg.adapter == "postgres" {
        let mut a = PostgresAdapter::new();
        a.connect(&cfg.database).await?;
        Ok(Box::new(a))
    } else {
        Err(anyhow!("adapter '{}' not supported", cfg.adapter))
    }
}

fn init_project() -> Result<()> {
    std::fs::create_dir_all("schema")?;
    std::fs::create_dir_all("data")?;
    std::fs::create_dir_all("migrations")?;
    std::fs::create_dir_all("queries")?;
    std::fs::create_dir_all("plugins")?;

    if !std::path::Path::new("kairo.config").exists() {
        let config = "# KairoDB Configuration\nadapter = \"sqlite\"\ndatabase = \"data/kairo.db\"\n";
        std::fs::write("kairo.config", config)?;
    }

    if !std::path::Path::new("schema/users.kairo").exists() {
        let sample_schema = "// Sample KairoDB schema\ntable users {\n  id: int [primary]\n  name: string [required]\n  email: string [unique]\n}\n";
        std::fs::write("schema/users.kairo", sample_schema)?;
    }

    if !std::path::Path::new("queries/README.md").exists() {
        std::fs::write("queries/README.md", "# Queries\n\nStore reusable query snippets here.\n")?;
    }

    ui::print_success("initialized. a starter schema and project folders are ready");
    Ok(())
}
