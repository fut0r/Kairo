//! End-to-end tests against real SQLite files.

use kairo_core::ErrorKind;
use kairo_core::db::{
    Connection, ConnectionTarget, PageRequest, SortSpec, SqliteTarget, TableKind, ValueKind,
};
use kairo_core::schema::{Dialect, generate_sql, parse_schema};
use kairo_core::services::query::{self, QueryOptions};
use kairo_core::services::safety::Risk;
use kairo_core::services::schema_apply::{self, PlanAction};
use kairo_core::services::{export, report, test_connection};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A scratch directory that is removed when the test ends.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let dir =
            std::env::temp_dir().join(format!("kairo-test-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    /// A file name with a space and a `#`, which broke the URL-style
    /// connection strings earlier releases built.
    fn db_path(&self) -> PathBuf {
        self.dir.join("my data #1.db")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn open(path: &PathBuf) -> Connection {
    Connection::open(&ConnectionTarget::Sqlite(SqliteTarget::create(path)))
        .await
        .unwrap()
}

async fn run(conn: &Connection, sql: &str) -> query::QueryOutcome {
    query::run_query(conn, sql, &QueryOptions::unlimited())
        .await
        .unwrap_or_else(|err| panic!("{sql}\nfailed: {err:?}"))
}

const SEED: &str = "
    CREATE TABLE users (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        email VARCHAR(255) UNIQUE,
        age INTEGER,
        score REAL,
        active BOOLEAN DEFAULT 1,
        avatar BLOB,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP
    );
    CREATE TABLE posts (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
        title TEXT NOT NULL DEFAULT 'untitled',
        views INTEGER DEFAULT 0
    );
    CREATE INDEX posts_user_idx ON posts (user_id);
    CREATE VIEW active_users AS SELECT id, name FROM users WHERE active = 1;

    WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 120)
    INSERT INTO users (id, name, email, age, score, active, avatar)
    SELECT i, 'user' || i, 'user' || i || '@example.com', 18 + (i % 50), i * 1.5, i % 2,
           CASE WHEN i % 10 = 0 THEN x'DEADBEEF' END
    FROM n;

    INSERT INTO posts (user_id, title, views) VALUES (1, 'hello', 10), (1, '50%_off!', 3), (2, 'second', 0);
";

async fn seeded(name: &str) -> (Scratch, Connection) {
    let scratch = Scratch::new(name);
    let conn = open(&scratch.db_path()).await;
    run(&conn, SEED).await;
    (scratch, conn)
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_reports_the_common_mistakes() {
    let scratch = Scratch::new("open");

    let missing = SqliteTarget::existing(scratch.dir.join("nope.db"));
    let err = Connection::open(&ConnectionTarget::Sqlite(missing))
        .await
        .err()
        .unwrap();
    assert_eq!(err.kind, ErrorKind::NotFound);

    let text_file = scratch.dir.join("notes.db");
    std::fs::write(
        &text_file,
        "this is plainly not a database, just some text ".repeat(40),
    )
    .unwrap();
    let err = Connection::open(&ConnectionTarget::Sqlite(SqliteTarget::existing(
        &text_file,
    )))
    .await
    .err()
    .unwrap();
    assert_eq!(err.kind, ErrorKind::InvalidDatabase, "{err:?}");

    let schema_file = scratch.dir.join("users.kairo");
    std::fs::write(&schema_file, "table users { a: int }").unwrap();
    let err = Connection::open(&ConnectionTarget::Sqlite(SqliteTarget::existing(
        &schema_file,
    )))
    .await
    .err()
    .unwrap();
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_info_describes_the_file() {
    let (scratch, conn) = seeded("info").await;
    let info = conn.info();
    assert_eq!(info.name, "my data #1.db");
    assert!(info.location.ends_with("my data #1.db"));
    assert!(!info.location.starts_with(r"\\?\"));
    assert!(info.server_version.starts_with("SQLite 3."));
    assert_eq!(info.workspace_key, format!("sqlite:{}", info.location));
    assert!(!info.read_only);

    let check = test_connection(&ConnectionTarget::Sqlite(SqliteTarget::existing(
        scratch.db_path(),
    )))
    .await
    .unwrap();
    assert_eq!(check.table_count, 3);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tables_and_views_are_listed_with_counts() {
    let (_scratch, conn) = seeded("list").await;
    let tables = conn.list_tables().await.unwrap();

    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["active_users", "posts", "users"]);
    assert_eq!(tables[0].kind, TableKind::View);
    assert_eq!(tables[2].kind, TableKind::Table);
    assert_eq!(tables[2].column_count, 8);
    assert_eq!(tables[2].row_count, Some(120));
    assert_eq!(tables[1].row_count, Some(3));
    assert_eq!(tables[0].row_count, Some(60));
    assert!(!tables[2].row_count_estimated);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_table_is_described_in_full() {
    let (_scratch, conn) = seeded("describe").await;

    let users = conn.describe_table("users").await.unwrap();
    let column = |name: &str| users.columns.iter().find(|c| c.name == name).unwrap();
    assert_eq!(column("id").primary_key_position, 1);
    assert_eq!(column("id").kairo_type, "int");
    assert!(!column("name").nullable);
    assert!(column("email").nullable);
    assert_eq!(column("email").data_type, "VARCHAR(255)");
    assert_eq!(column("email").kairo_type, "string");
    assert_eq!(column("score").kairo_type, "float");
    assert_eq!(column("active").kairo_type, "bool");
    assert_eq!(column("active").default_value.as_deref(), Some("1"));
    assert_eq!(column("avatar").kairo_type, "blob");
    assert_eq!(
        column("created_at").default_value.as_deref(),
        Some("CURRENT_TIMESTAMP")
    );

    assert_eq!(users.indexes.len(), 1);
    assert!(users.indexes[0].unique);
    assert_eq!(users.indexes[0].columns, ["email"]);
    assert_eq!(users.indexes[0].origin, "unique constraint");
    assert!(users.create_sql.starts_with("CREATE TABLE users ("));

    let posts = conn.describe_table("posts").await.unwrap();
    assert_eq!(posts.foreign_keys.len(), 1);
    let key = &posts.foreign_keys[0];
    assert_eq!(key.columns, ["user_id"]);
    assert_eq!(key.references_table, "users");
    assert_eq!(key.references_columns, ["id"]);
    assert_eq!(key.on_delete.as_deref(), Some("CASCADE"));
    assert_eq!(posts.indexes[0].name, "posts_user_idx");
    assert_eq!(posts.indexes[0].origin, "index");
    assert!(
        posts
            .create_sql
            .contains("CREATE INDEX posts_user_idx ON posts (user_id);")
    );

    // Names are matched the way SQLite matches them, and the catalog's
    // spelling is what comes back.
    assert_eq!(conn.describe_table("USERS").await.unwrap().name, "users");

    let view = conn.describe_table("active_users").await.unwrap();
    assert_eq!(view.kind, TableKind::View);
    assert_eq!(view.columns.len(), 2);

    let err = conn.describe_table("ghosts").await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rows_are_paged_and_typed() {
    let (_scratch, conn) = seeded("page").await;

    let first = conn
        .fetch_rows("users", &PageRequest::first(50))
        .await
        .unwrap();
    assert_eq!(first.rows.len(), 50);
    assert_eq!(first.total_rows, 120);
    assert_eq!(first.columns.len(), 8);
    assert_eq!(first.columns[2].data_type, "VARCHAR(255)");
    assert_eq!(first.sql, "SELECT * FROM \"users\" LIMIT 50 OFFSET 0");

    // This is the bug 0.4 had: anything that was not text came back "null".
    let row = &first.rows[0];
    assert_eq!((row[0].kind, row[0].text.as_str()), (ValueKind::Int, "1"));
    assert_eq!(
        (row[1].kind, row[1].text.as_str()),
        (ValueKind::Text, "user1")
    );
    assert_eq!((row[3].kind, row[3].text.as_str()), (ValueKind::Int, "19"));
    assert_eq!(
        (row[4].kind, row[4].text.as_str()),
        (ValueKind::Float, "1.5")
    );
    assert_eq!(
        (row[5].kind, row[5].text.as_str()),
        (ValueKind::Bool, "true")
    );
    assert_eq!(row[6].kind, ValueKind::Null);
    assert_eq!(first.rows[9][6].text, "<blob 4 bytes>");
    assert_eq!(first.rows[1][5].text, "false");

    let last = conn
        .fetch_rows(
            "users",
            &PageRequest {
                limit: 50,
                offset: 100,
                filter: None,
                sort: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(last.rows.len(), 20);
    assert_eq!(last.rows[0][0].text, "101");

    // The page size is clamped rather than trusted.
    let huge = conn
        .fetch_rows("users", &PageRequest::first(1_000_000))
        .await
        .unwrap();
    assert_eq!(huge.limit, 1000);
    assert_eq!(huge.rows.len(), 120);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rows_are_filtered_and_sorted_safely() {
    let (_scratch, conn) = seeded("filter").await;

    let request = |filter: &str| PageRequest {
        limit: 100,
        offset: 0,
        filter: Some(filter.to_string()),
        sort: None,
    };

    // user11 and user110 through user119.
    let found = conn.fetch_rows("users", &request("USER11")).await.unwrap();
    assert_eq!(found.total_rows, 11);
    assert!(found.sql.contains("LIKE ?1 ESCAPE '!'"));
    assert!(
        !found.sql.contains("USER11"),
        "the filter is bound, not spliced"
    );

    // Wildcards in the filter are literal text.
    assert_eq!(
        conn.fetch_rows("users", &request("%"))
            .await
            .unwrap()
            .total_rows,
        0
    );
    assert_eq!(
        conn.fetch_rows("users", &request("user_1"))
            .await
            .unwrap()
            .total_rows,
        0
    );
    assert_eq!(
        conn.fetch_rows("posts", &request("50%_off!"))
            .await
            .unwrap()
            .total_rows,
        1
    );
    // A quote in the filter is just a character.
    assert_eq!(
        conn.fetch_rows("users", &request("' OR 1=1 --"))
            .await
            .unwrap()
            .total_rows,
        0
    );
    // Blank filters are ignored.
    assert_eq!(
        conn.fetch_rows("users", &request("   "))
            .await
            .unwrap()
            .total_rows,
        120
    );

    let sorted = conn
        .fetch_rows(
            "users",
            &PageRequest {
                limit: 3,
                offset: 0,
                filter: None,
                sort: Some(SortSpec {
                    column: "score".into(),
                    descending: true,
                }),
            },
        )
        .await
        .unwrap();
    assert_eq!(sorted.rows[0][0].text, "120");
    assert!(sorted.sql.contains("ORDER BY \"score\" DESC"));

    let bad_sort = conn
        .fetch_rows(
            "users",
            &PageRequest {
                limit: 3,
                offset: 0,
                filter: None,
                sort: Some(SortSpec {
                    column: "id; DROP TABLE users".into(),
                    descending: false,
                }),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(bad_sort.kind, ErrorKind::InvalidInput);

    let injected = conn
        .fetch_rows("users\"; DROP TABLE users; --", &PageRequest::first(10))
        .await
        .unwrap_err();
    assert_eq!(injected.kind, ErrorKind::NotFound);
    assert_eq!(conn.count_rows("users").await.unwrap(), 120);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn queries_return_structured_results() {
    let (_scratch, conn) = seeded("query").await;

    let all = run(&conn, "SELECT id, name, score FROM users ORDER BY id").await;
    assert_eq!(all.row_count, 120);
    assert_eq!(all.columns.len(), 3);
    assert_eq!(all.columns[0].name, "id");
    assert_eq!(all.columns[0].data_type, "INTEGER");
    assert_eq!(all.rows[2][1].text, "user3");
    assert_eq!(all.risk, Risk::Read);
    assert_eq!(all.rows_affected, None);
    assert!(!all.truncated);

    let short = run(&conn, "from users where id = 3").await;
    assert!(short.translated);
    assert_eq!(short.executed_sql, "SELECT * from users where id = 3");
    assert_eq!(short.row_count, 1);

    let capped = query::run_query(
        &conn,
        "SELECT * FROM users",
        &QueryOptions {
            max_rows: 10,
            timeout: Some(Duration::from_secs(5)),
        },
    )
    .await
    .unwrap();
    assert_eq!(capped.row_count, 10);
    assert!(capped.truncated);

    // Expressions have no declared type; aggregates still come back typed.
    let computed = run(
        &conn,
        "SELECT COUNT(*) AS n, AVG(score) AS mean, 'x' || 'y' AS s FROM users",
    )
    .await;
    assert_eq!(computed.rows[0][0].kind, ValueKind::Int);
    assert_eq!(computed.rows[0][0].text, "120");
    assert_eq!(computed.rows[0][1].kind, ValueKind::Float);
    assert_eq!(computed.rows[0][2].text, "xy");

    // No rows, but the grid still needs its column headers.
    let empty = run(&conn, "SELECT id, name FROM users WHERE id < 0").await;
    assert_eq!(empty.row_count, 0);
    let names: Vec<&str> = empty.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "name"]);

    let big = run(&conn, "SELECT 9007199254740993 AS n").await;
    assert_eq!(big.rows[0][0].text, "9007199254740993");
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn query_errors_are_classified() {
    let (_scratch, conn) = seeded("errors").await;
    let options = QueryOptions::unlimited();

    let typo = query::run_query(&conn, "SELEC * FROM users", &options)
        .await
        .unwrap_err();
    // An unrecognised first word is still sent; the database explains it.
    assert_eq!(typo.kind, ErrorKind::Syntax, "{typo:?}");
    assert!(typo.message.contains("SELEC"), "{typo:?}");

    let unknown = query::run_query(&conn, "SELECT * FROM ghosts", &options)
        .await
        .unwrap_err();
    assert_eq!(unknown.kind, ErrorKind::Syntax);
    assert!(unknown.message.contains("no such table"), "{unknown:?}");

    let duplicate = query::run_query(
        &conn,
        "INSERT INTO users (id, name) VALUES (1, 'again')",
        &options,
    )
    .await
    .unwrap_err();
    assert_eq!(duplicate.kind, ErrorKind::Constraint, "{duplicate:?}");

    let empty = query::run_query(&conn, "  ", &options).await.unwrap_err();
    assert_eq!(empty.kind, ErrorKind::InvalidInput);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_and_scripts_report_what_changed() {
    let (_scratch, conn) = seeded("write").await;

    let insert = run(
        &conn,
        "INSERT INTO posts (user_id, title) VALUES (3, 'a'), (3, 'b')",
    )
    .await;
    assert_eq!(insert.rows_affected, Some(2));
    assert_eq!(insert.risk, Risk::Write);
    assert_eq!(insert.row_count, 0);

    let script = run(
        &conn,
        "CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('one'), ('two'); SELECT body FROM notes ORDER BY body",
    )
    .await;
    assert_eq!(script.statement_count, 3);
    assert_eq!(script.row_count, 2);
    assert_eq!(script.rows[1][0].text, "two");

    let removed = run(&conn, "DELETE FROM notes WHERE body = 'one'").await;
    assert_eq!(removed.rows_affected, Some(1));
    assert_eq!(removed.risk, Risk::Destructive);
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_transaction_does_not_poison_the_connection() {
    let (_scratch, conn) = seeded("txn").await;
    run(&conn, "CREATE TABLE notes (body TEXT)").await;

    let failed = query::run_query(
        &conn,
        "BEGIN; INSERT INTO notes VALUES ('kept?'); INSERT INTO ghosts VALUES (1); COMMIT;",
        &QueryOptions::unlimited(),
    )
    .await;
    assert!(failed.is_err());

    // Run enough statements to touch every pooled connection: none of them
    // may still be inside the failed transaction.
    for _ in 0..8 {
        let count = run(&conn, "SELECT COUNT(*) FROM notes").await;
        assert_eq!(
            count.rows[0][0].text, "0",
            "the failed script was rolled back"
        );
        run(&conn, "INSERT INTO notes VALUES ('x'); DELETE FROM notes;").await;
    }
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runaway_query_is_stopped_at_the_time_limit() {
    let (_scratch, conn) = seeded("timeout").await;

    let started = Instant::now();
    let err = query::run_query(
        &conn,
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) SELECT COUNT(*) FROM n",
        &QueryOptions {
            max_rows: 10,
            timeout: Some(Duration::from_millis(300)),
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.kind, ErrorKind::Timeout, "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(10));

    // The limit belonged to that statement only.
    for _ in 0..8 {
        assert_eq!(
            run(&conn, "SELECT COUNT(*) FROM users").await.rows[0][0].text,
            "120"
        );
    }
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_connection_refuses_writes() {
    let (scratch, conn) = seeded("readonly").await;
    conn.close().await;

    let target = ConnectionTarget::Sqlite(SqliteTarget::existing(scratch.db_path()).read_only());
    let conn = Connection::open(&target).await.unwrap();
    assert!(conn.info().read_only);
    assert_eq!(
        run(&conn, "SELECT COUNT(*) FROM users").await.rows[0][0].text,
        "120"
    );

    let err = query::run_query(&conn, "DELETE FROM users", &QueryOptions::unlimited())
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::PermissionDenied, "{err:?}");
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_schema_is_planned_then_applied_once() {
    let scratch = Scratch::new("apply");
    let conn = open(&scratch.db_path()).await;

    let schema = parse_schema(
        "table users {\n  id: int [primary]\n  email: string [required, unique]\n  active: bool = true\n}\n\
         table order {\n  id: int [primary]\n  total: float = 0.0\n  note: string = \"it's new\"\n}",
    )
    .unwrap();

    let plan = schema_apply::plan(&conn, &schema).await.unwrap();
    assert_eq!((plan.creates, plan.existing), (2, 0));
    assert!(plan.items.iter().all(|i| i.action == PlanAction::Create));
    assert!(plan.sql.contains("CREATE TABLE IF NOT EXISTS \"order\" ("));
    assert_eq!(plan.target_name, "my data #1.db");
    // Planning changes nothing.
    assert!(conn.list_tables().await.unwrap().is_empty());

    let applied = schema_apply::apply(&conn, &schema).await.unwrap();
    assert_eq!((applied.created, applied.unchanged), (2, 0));

    let users = conn.describe_table("users").await.unwrap();
    assert_eq!(users.columns[0].primary_key_position, 1);
    assert!(!users.columns[1].nullable);
    assert!(
        users
            .indexes
            .iter()
            .any(|i| i.unique && i.columns == ["email"])
    );

    // The defaults really work, including the one with an apostrophe.
    run(&conn, "INSERT INTO \"order\" (id) VALUES (1)").await;
    let row = run(&conn, "SELECT total, note FROM \"order\"").await;
    assert_eq!(row.rows[0][0].text, "0.0");
    assert_eq!(row.rows[0][1].text, "it's new");

    // Applying again is a no-op and says so.
    let again = schema_apply::apply(&conn, &schema).await.unwrap();
    assert_eq!((again.created, again.unchanged), (0, 2));
    assert!(again.items.iter().all(|i| i.differences.is_empty()));

    // An edited schema is not migrated; the plan names what differs.
    let edited =
        parse_schema("table users {\n  id: int [primary]\n  email: int\n  nickname: string\n}")
            .unwrap();
    let plan = schema_apply::plan(&conn, &edited).await.unwrap();
    assert_eq!(plan.items[0].action, PlanAction::Exists);
    let notes = plan.items[0].differences.join("\n");
    assert!(
        notes.contains("`email` is `int` in the schema but `text` in the database"),
        "{notes}"
    );
    assert!(
        notes.contains("`nickname` is in the schema but not in the database"),
        "{notes}"
    );
    assert!(
        notes.contains("`active` is in the database but not in the schema"),
        "{notes}"
    );
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_apply_changes_nothing() {
    let scratch = Scratch::new("atomic");
    let conn = open(&scratch.db_path()).await;

    // SQLite reserves the sqlite_ prefix, so the second table cannot be made.
    let schema = parse_schema("table good { a: int }\ntable sqlite_bad { a: int }").unwrap();
    let err = schema_apply::apply(&conn, &schema).await.unwrap_err();
    assert!(
        err.message.contains("failed at table `sqlite_bad`"),
        "{err:?}"
    );
    assert!(err.message.contains("Nothing was changed"));

    assert!(
        conn.list_tables().await.unwrap().is_empty(),
        "`good` was rolled back"
    );
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_export_can_be_applied_to_a_new_database() {
    let (scratch, conn) = seeded("export").await;

    let exported = export::export_schema(&conn, Some("data/app.db"))
        .await
        .unwrap();
    assert_eq!(exported.table_count, 2);
    assert!(
        exported
            .text
            .starts_with("// Generated by KairoDB\n// Source (SQLite): data/app.db\n\n")
    );
    for line in [
        "table users {",
        "  id: int [primary]",
        "  name: string [required]",
        "  email: string [unique] // native: varchar(255)",
        "  active: bool = true",
        "  avatar: blob",
        "  created_at: string // default: CURRENT_TIMESTAMP",
        "  user_id: int [required]",
        "  title: string = \"untitled\" [required]",
        "  views: int = 0",
    ] {
        assert!(
            exported.text.contains(line),
            "missing `{line}` in:\n{}",
            exported.text
        );
    }
    assert!(
        !exported.text.contains("active_users"),
        "views are not exported"
    );

    // 0.4 wrote files its own parser rejected. This one must parse and apply.
    let schema = parse_schema(&exported.text).unwrap();
    let copy = open(&scratch.dir.join("copy.db")).await;
    let applied = schema_apply::apply(&copy, &schema).await.unwrap();
    assert_eq!(applied.created, 2);

    let again = export::export_schema(&copy, None).await.unwrap();
    assert_eq!(
        generate_sql(&parse_schema(&again.text).unwrap(), Dialect::Sqlite),
        generate_sql(&schema, Dialect::Sqlite),
        "exporting the copy gives the same structure"
    );
    conn.close().await;
    copy.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_read_report_keeps_its_format() {
    let (_scratch, conn) = seeded("report").await;
    let text = report::read_report(&conn, Some("myapp.db")).await.unwrap();

    assert!(text.starts_with(&format!(
        "Database (SQLite): myapp.db\n{}\n\nTables: posts, users\n\n",
        "-".repeat(40)
    )));
    assert!(
        text.contains("table users {\n  id: int\n  name: string [required]\n"),
        "{text}"
    );
    assert!(
        text.contains("  title: string = 'untitled' [required]\n"),
        "{text}"
    );
    assert!(text.contains("}\n\n  -- 120 rows\n  | id: 1 | name: user1 | email: user1@example.com | age: 19 | score: 1.5 | active: true | avatar: null |"), "{text}");
    assert!(text.contains("  ... and 115 more rows\n"), "{text}");
    assert!(text.contains("  -- 3 rows\n"), "{text}");

    let scratch = Scratch::new("report-empty");
    let empty = open(&scratch.db_path()).await;
    let text = report::read_report(&empty, Some("empty.db")).await.unwrap();
    assert!(text.ends_with("(empty database)\n"));
    conn.close().await;
    empty.close().await;
}
