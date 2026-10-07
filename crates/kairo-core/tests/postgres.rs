//! End-to-end tests against a real PostgreSQL server.
//!
//! These run only when `KAIRO_TEST_POSTGRES_URL` is set, for example:
//!
//! ```text
//! KAIRO_TEST_POSTGRES_URL=postgres://postgres:postgres@localhost:5432/postgres cargo test -p kairo-core --test postgres
//! ```
//!
//! The test creates and drops tables whose names start with `kt_`. Point it
//! at a scratch database. Set `KAIRO_TEST_SKIP_TIMEOUT=1` for servers that
//! cannot interrupt a running statement.

use kairo_core::ErrorKind;
use kairo_core::db::{Connection, ConnectionTarget, PageRequest, SortSpec, TableKind, ValueKind};
use kairo_core::schema::parse_schema;
use kairo_core::services::query::{self, QueryOptions};
use kairo_core::services::safety::Risk;
use kairo_core::services::schema_apply::{self, PlanAction};
use kairo_core::services::{export, report};
use std::time::{Duration, Instant};

const CLEANUP: &str = "DROP VIEW IF EXISTS kt_active; \
                       DROP TABLE IF EXISTS kt_posts; \
                       DROP TABLE IF EXISTS kt_users; \
                       DROP TABLE IF EXISTS \"kt Order\"; \
                       DROP TABLE IF EXISTS kt_notes;";

const SCHEMA: &str = "table kt_users {
  id: int [primary]
  name: string [required]
  email: string [unique]
  score: float = 1.5
  active: bool = true
  avatar: blob
  joined: timestamp
  note: string = \"it's new\"
}

table \"kt Order\" {
  id: int [primary]
  \"Unit Price\": float [required]
}
";

const SEED: &str = "
    INSERT INTO kt_users (id, name, email, score, active, avatar, joined)
    SELECT i, 'user' || i, 'user' || i || '@example.com', i * 1.5, i % 2 = 1,
           CASE WHEN i % 10 = 0 THEN '\\xDEADBEEF'::bytea END,
           timestamp '2025-01-01 00:00:00' + (i || ' hours')::interval
      FROM generate_series(1, 120) AS g(i);

    CREATE TABLE kt_posts (
        id serial PRIMARY KEY,
        user_id integer NOT NULL REFERENCES kt_users(id) ON DELETE CASCADE,
        title varchar(80) NOT NULL DEFAULT 'untitled',
        meta jsonb,
        tags text[],
        uid uuid DEFAULT gen_random_uuid(),
        amount numeric(20, 2),
        created timestamptz DEFAULT now()
    );
    CREATE INDEX kt_posts_user_idx ON kt_posts (user_id);
    CREATE VIEW kt_active AS SELECT id, name FROM kt_users WHERE active;

    INSERT INTO kt_posts (user_id, title, meta, tags, amount) VALUES
        (1, 'hello', '{\"a\": 1}', ARRAY['x', 'y'], 12.50),
        (1, '50%_off!', NULL, NULL, 9007199254740.25),
        (2, 'second', NULL, NULL, NULL);
";

async fn run(conn: &Connection, sql: &str) -> query::QueryOutcome {
    query::run_query(conn, sql, &QueryOptions::unlimited())
        .await
        .unwrap_or_else(|err| panic!("{sql}\nfailed: {err:?}"))
}

fn page(limit: u32, filter: Option<&str>, sort: Option<(&str, bool)>) -> PageRequest {
    PageRequest {
        limit,
        offset: 0,
        filter: filter.map(str::to_string),
        sort: sort.map(|(column, descending)| SortSpec {
            column: column.to_string(),
            descending,
        }),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn postgres_end_to_end() {
    let Some(url) = std::env::var("KAIRO_TEST_POSTGRES_URL")
        .ok()
        .filter(|u| !u.is_empty())
    else {
        eprintln!("skipped: set KAIRO_TEST_POSTGRES_URL to run the PostgreSQL tests");
        return;
    };

    let target = ConnectionTarget::parse(&url).unwrap();
    let password = match &target {
        ConnectionTarget::Postgres(pg) if pg.has_password => url
            .split_once("://")
            .and_then(|(_, rest)| rest.split_once('@'))
            .and_then(|(userinfo, _)| userinfo.split_once(':'))
            .map(|(_, password)| password.to_string()),
        _ => None,
    };
    // The leak checks need a password that appears nowhere else in the URL.
    // With `postgres:postgres@…/postgres` the word is also the user and the
    // database, which are rightly shown.
    let distinctive = password.filter(|p| url.matches(p.as_str()).count() == 1);
    if distinctive.is_none() {
        eprintln!(
            "note: password leak checks are off; use a password unlike the user or database name"
        );
    }
    let leaks = |text: &str| distinctive.as_deref().is_some_and(|p| text.contains(p));

    let conn = Connection::open(&target).await.unwrap();
    run(&conn, CLEANUP).await;

    // ── Connection ──
    let info = conn.info();
    assert!(info.server_version.starts_with("PostgreSQL "), "{info:?}");
    assert!(info.host.is_some() && info.database.is_some() && info.username.is_some());
    assert!(info.workspace_key.starts_with("postgres:"));
    assert!(
        !leaks(&format!("{info:?}")),
        "connection info holds the password"
    );

    // ── Schema: plan, apply, apply again ──
    let schema = parse_schema(SCHEMA).unwrap();
    let plan = schema_apply::plan(&conn, &schema).await.unwrap();
    assert_eq!((plan.creates, plan.existing), (2, 0), "{plan:?}");
    assert!(
        plan.sql.contains("score DOUBLE PRECISION DEFAULT 1.5"),
        "{}",
        plan.sql
    );
    assert!(plan.sql.contains("avatar BYTEA"), "{}", plan.sql);
    assert!(
        plan.sql.contains("note TEXT DEFAULT 'it''s new'"),
        "{}",
        plan.sql
    );
    assert!(
        plan.sql
            .contains("CREATE TABLE IF NOT EXISTS \"kt Order\" ("),
        "{}",
        plan.sql
    );
    assert!(!leaks(&plan.target_location));

    let applied = schema_apply::apply(&conn, &schema).await.unwrap();
    assert_eq!((applied.created, applied.unchanged), (2, 0));

    let again = schema_apply::plan(&conn, &schema).await.unwrap();
    assert_eq!((again.creates, again.existing), (0, 2), "{again:?}");
    assert!(again.items.iter().all(|i| i.action == PlanAction::Exists));
    assert!(
        again.items.iter().all(|i| i.differences.is_empty()),
        "{again:?}"
    );

    // ── Seed through the query service ──
    let seeded = run(&conn, SEED).await;
    assert!(seeded.statement_count >= 5);
    assert_eq!(seeded.risk, Risk::Write);

    // ── Catalog ──
    let tables = conn.list_tables().await.unwrap();
    let find = |name: &str| {
        tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    };
    assert_eq!(find("kt_users").kind, TableKind::Table);
    assert_eq!(find("kt_users").column_count, 8);
    assert_eq!(find("kt_active").kind, TableKind::View);
    assert_eq!(find("kt Order").column_count, 2);
    assert_eq!(find("kt_posts").column_count, 8);

    let users = conn.describe_table("kt_users").await.unwrap();
    let column = |name: &str| users.columns.iter().find(|c| c.name == name).unwrap();
    assert_eq!(column("id").primary_key_position, 1);
    assert_eq!(column("id").data_type, "integer");
    assert!(!column("id").nullable && !column("name").nullable && column("email").nullable);
    assert_eq!(column("score").data_type, "double precision");
    assert_eq!(column("score").kairo_type, "float");
    assert_eq!(column("active").default_value.as_deref(), Some("true"));
    assert_eq!(column("avatar").kairo_type, "blob");
    assert_eq!(column("joined").kairo_type, "timestamp");
    assert!(
        column("note")
            .default_value
            .as_deref()
            .unwrap()
            .starts_with("'it''s new'")
    );
    assert!(
        users
            .indexes
            .iter()
            .any(|i| i.primary && i.columns == ["id"])
    );
    assert!(users.indexes.iter().any(|i| i.unique
        && !i.primary
        && i.columns == ["email"]
        && i.origin == "unique constraint"));
    assert!(
        users.create_sql.contains("CREATE TABLE \"kt_users\" ("),
        "{}",
        users.create_sql
    );
    assert!(
        users.create_sql.contains("PRIMARY KEY (\"id\")"),
        "{}",
        users.create_sql
    );

    let posts = conn.describe_table("kt_posts").await.unwrap();
    assert_eq!(posts.foreign_keys.len(), 1, "{posts:?}");
    let key = &posts.foreign_keys[0];
    assert_eq!(
        (key.columns.as_slice(), key.references_table.as_str()),
        (["user_id".to_string()].as_slice(), "kt_users")
    );
    assert_eq!(key.references_columns, ["id"]);
    assert_eq!(key.on_delete.as_deref(), Some("CASCADE"));
    assert!(
        posts.indexes.iter().any(|i| i.name == "kt_posts_user_idx"
            && i.origin == "index"
            && i.columns == ["user_id"])
    );
    assert!(
        posts.create_sql.contains("CREATE INDEX kt_posts_user_idx"),
        "{}",
        posts.create_sql
    );
    assert_eq!(
        posts
            .columns
            .iter()
            .find(|c| c.name == "title")
            .unwrap()
            .data_type,
        "character varying(80)"
    );

    let view = conn.describe_table("kt_active").await.unwrap();
    assert_eq!(view.kind, TableKind::View);
    assert!(
        view.create_sql.starts_with("CREATE VIEW \"kt_active\" AS"),
        "{}",
        view.create_sql
    );
    assert_eq!(
        conn.describe_table("kt_ghosts").await.unwrap_err().kind,
        ErrorKind::NotFound
    );

    // ── Browsing ──
    let first = conn
        .fetch_rows("kt_users", &page(50, None, None))
        .await
        .unwrap();
    assert_eq!((first.rows.len(), first.total_rows), (50, 120));
    assert!(
        first.sql.contains("ORDER BY \"kt_users\".\"id\""),
        "pages are ordered by the key column, not its text alias: {}",
        first.sql
    );
    // Numeric order: 1, 2, 3 … not the text order 1, 10, 100.
    let ids: Vec<&str> = first
        .rows
        .iter()
        .take(11)
        .map(|r| r[0].text.as_str())
        .collect();
    assert_eq!(
        ids,
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"]
    );
    let row = &first.rows[0];
    assert_eq!((row[0].kind, row[0].text.as_str()), (ValueKind::Int, "1"));
    assert_eq!(
        (row[1].kind, row[1].text.as_str()),
        (ValueKind::Text, "user1")
    );
    assert_eq!(
        (row[3].kind, row[3].text.as_str()),
        (ValueKind::Float, "1.5")
    );
    assert_eq!(
        (row[4].kind, row[4].text.as_str()),
        (ValueKind::Bool, "true")
    );
    assert_eq!(row[5].kind, ValueKind::Null);
    assert_eq!(first.rows[9][5].text, "<blob 4 bytes>");
    assert_eq!(first.rows[1][4].text, "false");
    assert_eq!(row[6].text, "2025-01-01 01:00:00");

    let found = conn
        .fetch_rows("kt_users", &page(100, Some("USER11"), None))
        .await
        .unwrap();
    assert_eq!(found.total_rows, 11, "ILIKE across columns");
    assert!(found.sql.contains("ILIKE $1 ESCAPE '!'") && !found.sql.contains("USER11"));
    assert_eq!(
        conn.fetch_rows("kt_users", &page(10, Some("%"), None))
            .await
            .unwrap()
            .total_rows,
        0
    );
    assert_eq!(
        conn.fetch_rows("kt_posts", &page(10, Some("50%_off!"), None))
            .await
            .unwrap()
            .total_rows,
        1
    );
    assert_eq!(
        conn.fetch_rows("kt_users", &page(10, Some("' OR 1=1 --"), None))
            .await
            .unwrap()
            .total_rows,
        0
    );

    let sorted = conn
        .fetch_rows("kt_users", &page(3, None, Some(("score", true))))
        .await
        .unwrap();
    assert_eq!(sorted.rows[0][0].text, "120");
    let bad = conn
        .fetch_rows(
            "kt_users",
            &page(3, None, Some(("id; DROP TABLE kt_users", false))),
        )
        .await
        .unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidInput);
    let injected = conn
        .fetch_rows("kt_users\"; DROP TABLE kt_users; --", &page(3, None, None))
        .await
        .unwrap_err();
    assert_eq!(injected.kind, ErrorKind::NotFound);

    // Types with no decoder of their own are still shown, as the server prints them.
    let rich = conn
        .fetch_rows("kt_posts", &page(10, None, None))
        .await
        .unwrap();
    let names: Vec<&str> = rich.columns.iter().map(|c| c.name.as_str()).collect();
    let at = |name: &str| names.iter().position(|n| *n == name).unwrap();
    assert_eq!(rich.rows[0][at("meta")].text, "{\"a\": 1}");
    assert_eq!(rich.rows[0][at("tags")].text, "{x,y}");
    assert_eq!(rich.rows[0][at("uid")].text.len(), 36);
    assert_eq!(rich.rows[0][at("amount")].text, "12.50");
    assert_eq!(rich.rows[1][at("amount")].text, "9007199254740.25");
    assert_eq!(rich.rows[0][at("amount")].kind, ValueKind::Float);
    assert!(rich.rows[0][at("created")].text.starts_with("20"));
    assert_eq!(rich.rows[1][at("meta")].kind, ValueKind::Null);

    let quoted = conn
        .fetch_rows("kt Order", &page(10, None, None))
        .await
        .unwrap();
    assert_eq!(quoted.total_rows, 0);
    assert_eq!(quoted.columns[1].name, "Unit Price");

    // ── Queries ──
    let all = run(
        &conn,
        "SELECT id, name, score, active, avatar FROM kt_users ORDER BY id",
    )
    .await;
    assert_eq!(all.row_count, 120);
    assert_eq!(all.columns[0].data_type, "int4");
    assert_eq!(
        (all.rows[0][0].kind, all.rows[0][0].text.as_str()),
        (ValueKind::Int, "1")
    );
    assert_eq!(
        (all.rows[0][2].kind, all.rows[0][2].text.as_str()),
        (ValueKind::Float, "1.5")
    );
    assert_eq!(
        (all.rows[0][3].kind, all.rows[0][3].text.as_str()),
        (ValueKind::Bool, "true")
    );
    assert_eq!(all.rows[9][4].text, "<blob 4 bytes>");
    assert_eq!(all.rows_affected, None);

    let typed = run(
        &conn,
        "SELECT 9007199254740993::bigint AS n, 12.50::numeric AS d, gen_random_uuid() AS u, \
                '{\"k\": [1, 2]}'::jsonb AS j, ARRAY[1, 2, 3] AS a, NULL::text AS z, now() AS t",
    )
    .await;
    let cells = &typed.rows[0];
    assert_eq!(
        cells[0].text, "9007199254740993",
        "64-bit integers keep every digit"
    );
    assert_eq!(cells[1].text, "12.50");
    assert_eq!(cells[2].text.len(), 36);
    assert_eq!(cells[3].text, "{\"k\": [1, 2]}");
    assert_eq!(cells[4].text, "{1,2,3}");
    assert_eq!(cells[5].kind, ValueKind::Null);
    assert!(cells[6].text.starts_with("20"));

    let short = run(&conn, "from kt_users where id = 3").await;
    assert!(short.translated);
    assert_eq!(short.row_count, 1);

    let capped = query::run_query(
        &conn,
        "SELECT * FROM kt_users",
        &QueryOptions {
            max_rows: 10,
            timeout: Some(Duration::from_secs(10)),
        },
    )
    .await
    .unwrap();
    assert_eq!(capped.row_count, 10);
    assert!(capped.truncated);
    // The connection is still usable after a result was cut short.
    assert_eq!(
        run(&conn, "SELECT count(*) FROM kt_users").await.rows[0][0].text,
        "120"
    );

    let empty = run(&conn, "SELECT id, name FROM kt_users WHERE id < 0").await;
    assert_eq!(empty.row_count, 0);
    let headers: Vec<&str> = empty.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(headers, ["id", "name"]);

    let script = run(
        &conn,
        "CREATE TABLE kt_notes (body text); INSERT INTO kt_notes VALUES ('one'), ('two'); \
         SELECT body FROM kt_notes ORDER BY body",
    )
    .await;
    assert_eq!(script.statement_count, 3);
    assert_eq!(script.rows[1][0].text, "two");
    assert_eq!(
        run(&conn, "DELETE FROM kt_notes WHERE body = 'one'")
            .await
            .rows_affected,
        Some(1)
    );

    // ── Errors ──
    let options = QueryOptions::unlimited();
    let typo = query::run_query(&conn, "SELEC 1", &options)
        .await
        .unwrap_err();
    assert_eq!(typo.kind, ErrorKind::Syntax, "{typo:?}");
    let unknown = query::run_query(&conn, "SELECT * FROM kt_ghosts", &options)
        .await
        .unwrap_err();
    assert_eq!(unknown.kind, ErrorKind::Syntax, "{unknown:?}");
    let duplicate = query::run_query(
        &conn,
        "INSERT INTO kt_users (id, name) VALUES (1, 'again')",
        &options,
    )
    .await
    .unwrap_err();
    assert_eq!(duplicate.kind, ErrorKind::Constraint, "{duplicate:?}");

    // A failure inside BEGIN must not leave the pooled connection in an
    // aborted transaction.
    let failed = query::run_query(
        &conn,
        "BEGIN; INSERT INTO kt_notes VALUES ('kept?'); SELECT 1 / 0; COMMIT;",
        &options,
    )
    .await;
    assert!(failed.is_err());
    for _ in 0..6 {
        assert_eq!(
            run(&conn, "SELECT count(*) FROM kt_notes").await.rows[0][0].text,
            "1"
        );
    }

    if std::env::var_os("KAIRO_TEST_SKIP_TIMEOUT").is_none() {
        let started = Instant::now();
        let slow = query::run_query(
            &conn,
            "SELECT pg_sleep(30)",
            &QueryOptions {
                max_rows: 10,
                timeout: Some(Duration::from_millis(400)),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(slow.kind, ErrorKind::Timeout, "{slow:?}");
        assert!(started.elapsed() < Duration::from_secs(15));
        // The limit applied to that statement only.
        assert_eq!(
            run(&conn, "SELECT count(*) FROM kt_users").await.rows[0][0].text,
            "120"
        );
    }

    // A failed apply changes nothing: the second table collides with a view.
    let colliding = parse_schema("table kt_fresh { a: int }\ntable kt_active { a: int }").unwrap();
    let plan = schema_apply::plan(&conn, &colliding).await.unwrap();
    assert!(
        plan.items[1].differences[0].contains("already exists as a view"),
        "{plan:?}"
    );

    // ── Export and report ──
    let exported = export::export_schema(&conn, None).await.unwrap();
    assert!(
        !leaks(&exported.text),
        "the export header holds the password"
    );
    for line in [
        "table kt_users {",
        "  id: int [primary]",
        "  name: string [required]",
        "  email: string [unique]",
        "  score: float = 1.5",
        "  active: bool = true",
        "  avatar: blob",
        "  note: string = \"it's new\"",
        "table \"kt Order\" {",
        "  \"Unit Price\": float [required]",
        "  title: string = \"untitled\" [required] // native: character varying(80)",
    ] {
        assert!(
            exported.text.contains(line),
            "missing `{line}` in:\n{}",
            exported.text
        );
    }
    assert!(
        !exported.text.contains("kt_active"),
        "views are not exported"
    );
    // What 0.4 could not do: the export parses.
    let reparsed = parse_schema(&exported.text).unwrap();
    assert!(
        reparsed
            .tables
            .iter()
            .any(|t| t.name == "kt Order" && t.quoted)
    );

    let text = report::read_report(&conn, Some(&url)).await.unwrap();
    assert!(
        text.starts_with("Database (PostgreSQL): postgres"),
        "{text}"
    );
    assert!(!leaks(&text), "the report holds the password");
    assert!(text.contains("  -- 120 rows\n"), "{text}");
    assert!(text.contains("  | id: 1 | name: user1 |"), "{text}");
    assert!(text.contains("  ... and 115 more rows\n"), "{text}");

    run(&conn, CLEANUP).await;
    conn.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_servers_fail_fast_and_without_the_password() {
    // Port 1 is never a PostgreSQL server, so this needs no setup.
    let target = ConnectionTarget::parse("postgres://kairo:hunter2@127.0.0.1:1/app").unwrap();
    let started = Instant::now();
    let err = Connection::open(&target).await.err().unwrap();

    assert!(
        matches!(err.kind, ErrorKind::Network | ErrorKind::Timeout),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(!format!("{err:?} {err}").contains("hunter2"));
}
