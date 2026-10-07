use crate::ui;
use clap::{Parser, Subcommand};
use kairo_core::db::{Connection, ConnectionTarget, SqliteTarget, TableKind, is_postgres_url};
use kairo_core::redact::redact;
use kairo_core::schema::{self, Dialect};
use kairo_core::services::schema_apply::{self, PlanAction};
use kairo_core::services::{export, project, query, report};
use kairo_core::{ErrorKind, KairoError, Result};
use std::path::Path;

#[derive(Parser)]
#[command(name = "kairo")]
#[command(version)]
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
        /// Path to the database file, or a PostgreSQL URL
        file: String,
    },

    /// Export a database file to .kairo schema format
    Export {
        /// Path to the database file, or a PostgreSQL URL
        file: String,
        /// Output file (optional, prints to stdout if not given)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// List all tables in the current database
    Tables,

    /// Show current project status
    Status,
}

pub(crate) fn print_failure(err: &KairoError) {
    if err.kind == ErrorKind::InvalidDatabase {
        ui::print_error("the file is not a valid database format or is corrupted.");
        return;
    }
    ui::print_error(&err.to_string());
    if let Some(hint) = &err.hint {
        ui::print_hint(hint);
    }
}

/// The project in the current directory. An empty path, not `.`, so that
/// paths shown to the user read `schema/users.kairo` and not `./schema/…`.
fn project_dir() -> &'static Path {
    Path::new("")
}

async fn connect_project() -> Result<(project::Config, Connection)> {
    let config = project::load_config(project_dir())?;
    let target = project::target_from_config(&config, project_dir())?;
    let conn = Connection::open(&target).await?;
    Ok((config, conn))
}

/// Resolves the argument of `read` and `export`, with the messages those
/// commands have always given for the usual mistakes.
fn inspect_target(file: &str, purpose: &str) -> Result<ConnectionTarget> {
    if is_postgres_url(file) {
        return ConnectionTarget::parse(file);
    }

    let path = Path::new(file);
    if path.extension().is_some_and(|ext| ext == "kairo") {
        return Err(KairoError::invalid_input(format!(
            "file '{file}' is a .kairo schema file. {purpose}"
        )));
    }

    if !path.exists() {
        let mut message = format!("database file '{file}' not found.");
        if file == "kairo.db" && Path::new("data/kairo.db").exists() {
            message.push_str(" Did you mean 'data/kairo.db'?");
        }
        return Err(KairoError::not_found(message));
    }
    if !path.is_file() {
        return Err(KairoError::invalid_input(format!(
            "'{file}' is not a file."
        )));
    }

    // Inspection never writes, so open the file read-only.
    Ok(ConnectionTarget::Sqlite(
        SqliteTarget::existing(path).read_only(),
    ))
}

pub(crate) async fn run() -> Result<()> {
    match Cli::parse().command {
        Some(Commands::Init) => {
            project::init_project(project_dir())?;
            ui::print_success("initialized.");
        }

        Some(Commands::Create { name }) => {
            let schema_path = project::schema_path(project_dir(), &name);
            let content = std::fs::read_to_string(&schema_path).map_err(|_| {
                KairoError::not_found(format!("no schema file at {}", schema_path.display()))
            })?;
            let parsed = schema::parse_schema(&content)?;

            let config = project::load_config(project_dir()).ok();
            let dialect = match config.as_ref().map(|c| c.adapter.as_str()) {
                Some("postgres" | "postgresql") => Dialect::Postgres,
                _ => Dialect::Sqlite,
            };

            ui::print_dim("sql:");
            println!("{}", schema::generate_sql(&parsed, dialect));

            let Some(config) = config else {
                ui::print_dim("no kairo.config found, schema not applied to any database");
                return Ok(());
            };

            let target = project::target_from_config(&config, project_dir())?;
            let conn = Connection::open(&target).await?;
            let applied = schema_apply::apply(&conn, &parsed).await;
            conn.close().await;
            let applied = applied?;

            // An existing table is never altered; say so instead of implying
            // the schema was applied to it.
            for item in applied
                .items
                .iter()
                .filter(|i| i.action == PlanAction::Exists)
            {
                ui::print_dim(&format!(
                    "table '{}' already exists, left unchanged",
                    item.table
                ));
                for difference in &item.differences {
                    ui::print_dim(&format!("  {difference}"));
                }
            }

            ui::print_success(&format!(
                "applied schema '{}' to {}",
                name,
                redact(&config.database)
            ));
        }

        Some(Commands::Query { query }) => {
            let (_, conn) = connect_project().await?;
            let result = query::run_query(&conn, &query, &query::QueryOptions::unlimited()).await;
            conn.close().await;
            let result = result?;

            if result.translated {
                ui::print_dim(&format!("-> {}", result.executed_sql));
            }

            if result.rows.is_empty() {
                match result.rows_affected {
                    Some(count) => ui::print_dim(&format!("{count} rows affected")),
                    None => ui::print_dim("(no rows)"),
                }
            } else {
                let headers: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
                println!("{}", headers.join(" | "));
                println!(
                    "{}",
                    "-".repeat(headers.iter().map(|h| h.len() + 3).sum::<usize>())
                );

                for row in &result.rows {
                    let fields: Vec<&str> = row.iter().map(|value| value.display()).collect();
                    println!("{}", fields.join(" | "));
                }
                ui::print_dim(&format!("{} rows", result.rows.len()));
            }
        }

        Some(Commands::Read { file }) => {
            let target = inspect_target(
                &file,
                "'kairo read' is used to inspect SQLite database files (e.g., 'data/kairo.db').",
            )?;
            let conn = Connection::open(&target).await?;
            let output = report::read_report(&conn, Some(&file)).await;
            conn.close().await;
            print!("{}", output?);
        }

        Some(Commands::Export { file, output }) => {
            let target = inspect_target(
                &file,
                "'kairo export' is used to convert SQLite database files (e.g., 'data/kairo.db') to schema format.",
            )?;
            let conn = Connection::open(&target).await?;
            let exported = export::export_schema(&conn, Some(&file)).await;
            conn.close().await;
            let exported = exported?;

            match output {
                Some(path) => {
                    std::fs::write(&path, &exported.text)
                        .map_err(|err| KairoError::io(format!("could not write {path}."), &err))?;
                    ui::print_success(&format!("exported schema to {path}"));
                }
                None => print!("{}", exported.text),
            }
        }

        Some(Commands::Tables) => {
            let (_, conn) = connect_project().await?;
            let tables = conn.list_tables().await;
            conn.close().await;

            let tables: Vec<_> = tables?
                .into_iter()
                .filter(|t| t.kind == TableKind::Table)
                .collect();

            if tables.is_empty() {
                ui::print_dim("(no tables)");
            } else {
                ui::print_header("tables");
                for table in &tables {
                    ui::print_row("-", &table.name);
                }
            }
        }

        Some(Commands::Status) => {
            ui::print_header(&format!("kairo v{}", kairo_core::VERSION));

            let status = project::status(project_dir());
            if !status.initialized {
                match &status.problem {
                    Some(problem) => ui::print_error(problem),
                    None => ui::print_dim("not initialized. run 'kairo init'."),
                }
                return Ok(());
            }

            ui::print_row("adapter ", status.adapter.as_deref().unwrap_or(""));
            ui::print_row("database", status.database.as_deref().unwrap_or(""));
            ui::print_row("schemas ", &status.schema_files.len().to_string());
            if let Some(exists) = status.database_exists {
                ui::print_row(
                    "db file ",
                    if exists { "exists" } else { "not created yet" },
                );
            }
        }

        None => {
            ui::print_welcome();
        }
    }

    Ok(())
}
