//! Decides how risky a piece of SQL is before it runs.
//!
//! The analysis is lexical: it reads keywords, skipping comments, strings and
//! quoted names. It cannot see what a function does, so `SELECT purge()` is a
//! read here. When it does not recognise a statement it assumes the worst.

use crate::error::{ErrorKind, KairoError, Result};
use serde::{Deserialize, Serialize};

/// Ordered from harmless to dangerous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Only reads.
    Read,
    /// Adds data or objects, or changes settings.
    Write,
    /// Can remove or overwrite existing data, structure or permissions.
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementInfo {
    /// The leading keyword, upper case.
    pub keyword: String,
    pub risk: Risk,
    /// Why the statement is above `read`.
    pub reason: Option<String>,
    /// The start of the statement, on one line.
    pub preview: String,
    /// False when the leading keyword is not one Kairo knows. Such a
    /// statement is treated as destructive because it cannot be ruled out,
    /// not because it is known to be; it is often just a typo.
    pub recognized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlAnalysis {
    pub statements: Vec<StatementInfo>,
    /// The highest risk of any statement.
    pub risk: Risk,
    /// Distinct reasons, most severe first.
    pub reasons: Vec<String>,
    /// The last statement is expected to produce a result set.
    pub returns_rows: bool,
    /// A transaction is opened and not closed within the script.
    pub unbalanced_transaction: bool,
}

struct Statement {
    words: Vec<String>,
    text: String,
    has_equals: bool,
}

impl Statement {
    fn new() -> Self {
        Self {
            words: Vec::new(),
            text: String::new(),
            has_equals: false,
        }
    }

    fn has(&self, word: &str) -> bool {
        self.words.iter().any(|w| w == word)
    }

    fn word(&self, index: usize) -> &str {
        self.words.get(index).map(String::as_str).unwrap_or("")
    }

    /// True when `first` is immediately followed by `second`.
    fn has_pair(&self, first: &str, second: &str) -> bool {
        self.words
            .windows(2)
            .any(|pair| pair[0] == first && pair[1] == second)
    }
}

/// Splits a script into statements, keeping only what matters for
/// classification: keywords outside comments, strings and quoted names.
fn split_statements(sql: &str) -> Vec<Statement> {
    let chars: Vec<char> = sql.chars().collect();
    let len = chars.len();
    let mut statements = Vec::new();
    let mut current = Statement::new();
    let mut word = String::new();
    let mut i = 0;

    fn flush(word: &mut String, statement: &mut Statement) {
        if !word.is_empty() {
            statement.words.push(word.to_ascii_uppercase());
            word.clear();
        }
    }

    while i < len {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');

        // -- line comment
        if c == '-' && next == '-' {
            flush(&mut word, &mut current);
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            current.text.push(' ');
            continue;
        }

        // /* block comment */, which PostgreSQL allows to nest
        if c == '/' && next == '*' {
            flush(&mut word, &mut current);
            let mut depth = 1;
            i += 2;
            while i < len && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            current.text.push(' ');
            continue;
        }

        // 'string', with '' as an escaped quote. A PostgreSQL E'string' also
        // escapes with a backslash.
        if c == '\'' {
            let backslash_escapes = word.eq_ignore_ascii_case("e");
            if backslash_escapes {
                word.clear();
            } else {
                flush(&mut word, &mut current);
            }
            current.text.push(c);
            i += 1;
            while i < len {
                let ch = chars[i];
                current.text.push(ch);
                i += 1;
                if backslash_escapes && ch == '\\' && i < len {
                    current.text.push(chars[i]);
                    i += 1;
                } else if ch == '\'' {
                    if chars.get(i) == Some(&'\'') {
                        current.text.push('\'');
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        // "quoted name" or `quoted name`
        if c == '"' || c == '`' {
            flush(&mut word, &mut current);
            current.text.push(c);
            i += 1;
            while i < len {
                let ch = chars[i];
                current.text.push(ch);
                i += 1;
                if ch == c {
                    if chars.get(i) == Some(&c) {
                        current.text.push(c);
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        // $tag$ dollar-quoted string $tag$ (PostgreSQL function bodies)
        if c == '$' && word.is_empty() {
            let mut end = i + 1;
            while end < len && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let tag_ok = end < len
                && chars[end] == '$'
                && !chars.get(i + 1).is_some_and(|ch| ch.is_ascii_digit());
            if tag_ok {
                let delimiter: Vec<char> = chars[i..=end].to_vec();
                let body_start = end + 1;
                let close = (body_start..len).find(|&at| chars[at..].starts_with(&delimiter));
                let stop = close.map_or(len, |at| at + delimiter.len());
                current.text.extend(&chars[i..stop]);
                i = stop;
                continue;
            }
        }

        if c == ';' {
            flush(&mut word, &mut current);
            if !current.words.is_empty() {
                statements.push(std::mem::replace(&mut current, Statement::new()));
            } else {
                current = Statement::new();
            }
            i += 1;
            continue;
        }

        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut current);
            if c == '=' {
                current.has_equals = true;
            }
        }
        current.text.push(c);
        i += 1;
    }

    flush(&mut word, &mut current);
    if !current.words.is_empty() {
        statements.push(current);
    }
    statements
}

/// SQLite pragmas that only report something when called with an argument.
const READ_PRAGMAS: [&str; 14] = [
    "TABLE_INFO",
    "TABLE_XINFO",
    "TABLE_LIST",
    "INDEX_LIST",
    "INDEX_INFO",
    "INDEX_XINFO",
    "FOREIGN_KEY_LIST",
    "FOREIGN_KEY_CHECK",
    "INTEGRITY_CHECK",
    "QUICK_CHECK",
    "DATABASE_LIST",
    "COLLATION_LIST",
    "FUNCTION_LIST",
    "MODULE_LIST",
];

fn classify(statement: &Statement) -> (Risk, Option<String>) {
    let reason = |text: &str| Some(text.to_string());

    match statement.word(0) {
        "SELECT" | "VALUES" | "TABLE" | "SHOW" => {
            if statement.word(0) == "SELECT" && statement.has("INTO") {
                (Risk::Write, reason("SELECT … INTO creates a new table."))
            } else {
                (Risk::Read, None)
            }
        }

        "WITH" => {
            if statement.has("DELETE") || statement.has("UPDATE") || statement.has("MERGE") {
                (
                    Risk::Destructive,
                    reason("This WITH query changes or removes existing rows."),
                )
            } else if statement.has("INSERT") {
                (Risk::Write, reason("This WITH query inserts rows."))
            } else {
                (Risk::Read, None)
            }
        }

        "EXPLAIN" => {
            // EXPLAIN ANALYZE executes the statement it explains.
            let runs = statement.has("ANALYZE") || statement.has("ANALYSE");
            let inner_at = statement.words.iter().position(|w| {
                matches!(
                    w.as_str(),
                    "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "WITH" | "CREATE"
                )
            });
            match (runs, inner_at) {
                (true, Some(at)) => {
                    let inner = Statement {
                        words: statement.words[at..].to_vec(),
                        text: String::new(),
                        has_equals: statement.has_equals,
                    };
                    match classify(&inner) {
                        (Risk::Read, _) => (Risk::Read, None),
                        (risk, _) => (
                            risk,
                            reason("EXPLAIN ANALYZE really runs the statement it explains."),
                        ),
                    }
                }
                _ => (Risk::Read, None),
            }
        }

        "PRAGMA" => {
            let name = statement.word(1);
            let called_with_argument = statement.words.len() > 2;
            if statement.has_equals || (called_with_argument && !READ_PRAGMAS.contains(&name)) {
                (
                    Risk::Write,
                    reason("This PRAGMA changes a database setting."),
                )
            } else {
                (Risk::Read, None)
            }
        }

        "BEGIN" | "START" | "COMMIT" | "END" | "ROLLBACK" | "ABORT" | "SAVEPOINT" | "RELEASE" => {
            (Risk::Read, None)
        }

        "INSERT" => {
            if statement.has_pair("OR", "REPLACE") {
                (
                    Risk::Destructive,
                    reason("INSERT OR REPLACE overwrites rows that conflict."),
                )
            } else if statement.has_pair("DO", "UPDATE") {
                (
                    Risk::Destructive,
                    reason("ON CONFLICT … DO UPDATE overwrites existing rows."),
                )
            } else {
                (Risk::Write, reason("INSERT adds rows."))
            }
        }

        "REPLACE" => (
            Risk::Destructive,
            reason("REPLACE overwrites rows that conflict."),
        ),

        "UPDATE" => {
            if statement.has("WHERE") {
                (Risk::Destructive, reason("UPDATE changes existing rows."))
            } else {
                (
                    Risk::Destructive,
                    reason("UPDATE without WHERE changes every row in the table."),
                )
            }
        }

        "DELETE" => {
            if statement.has("WHERE") {
                (Risk::Destructive, reason("DELETE removes rows."))
            } else {
                (
                    Risk::Destructive,
                    reason("DELETE without WHERE removes every row in the table."),
                )
            }
        }

        "DROP" => {
            let object = statement.word(1).to_ascii_lowercase();
            let object = if object.is_empty() {
                "object".to_string()
            } else {
                object
            };
            (
                Risk::Destructive,
                Some(format!(
                    "DROP permanently removes the {object} and what it holds."
                )),
            )
        }

        "TRUNCATE" => (
            Risk::Destructive,
            reason("TRUNCATE removes every row in the table."),
        ),

        "ALTER" => (
            Risk::Destructive,
            reason("ALTER changes the structure of something that already exists."),
        ),

        "MERGE" => (
            Risk::Destructive,
            reason("MERGE can change or remove existing rows."),
        ),

        "GRANT" | "REVOKE" => (
            Risk::Destructive,
            reason("This changes who may access what."),
        ),

        "CREATE" => {
            if statement.has_pair("OR", "REPLACE") {
                (
                    Risk::Destructive,
                    reason("CREATE OR REPLACE overwrites an existing definition."),
                )
            } else {
                (Risk::Write, reason("CREATE adds a new object."))
            }
        }

        "VACUUM" | "REINDEX" | "ANALYZE" | "ANALYSE" | "ATTACH" | "DETACH" | "SET" | "RESET"
        | "COMMENT" | "COPY" | "REFRESH" | "CLUSTER" | "CHECKPOINT" | "LOCK" | "DISCARD"
        | "LISTEN" | "NOTIFY" | "UNLISTEN" | "PREPARE" | "DEALLOCATE" | "DECLARE" | "FETCH"
        | "MOVE" | "CLOSE" => (
            Risk::Write,
            Some(format!(
                "{} changes the database or this session.",
                statement.word(0)
            )),
        ),

        // Unknown. Destructive with no reason is how `analyze` tells this
        // apart from the statements above, which all explain themselves.
        _ => (Risk::Destructive, None),
    }
}

fn preview(text: &str) -> String {
    const LIMIT: usize = 140;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{}…", &collapsed[..cut]),
        None => collapsed,
    }
}

/// Classifies every statement in `sql`.
pub fn analyze(sql: &str) -> SqlAnalysis {
    let parsed = split_statements(sql);

    let mut statements = Vec::with_capacity(parsed.len());
    let mut depth: i32 = 0;

    for statement in &parsed {
        match statement.word(0) {
            "BEGIN" | "START" => depth += 1,
            "COMMIT" | "END" | "ABORT" => depth = (depth - 1).max(0),
            // `ROLLBACK TO name` returns to a savepoint; it does not end the transaction.
            "ROLLBACK" if statement.word(1) != "TO" => depth = (depth - 1).max(0),
            _ => {}
        }

        let (risk, reason) = classify(statement);
        let recognized = !(risk == Risk::Destructive && reason.is_none());
        let reason = reason.or_else(|| {
            (!recognized).then(|| {
                format!(
                    "Kairo does not recognise {}, so it asks before running it.",
                    statement.word(0)
                )
            })
        });
        statements.push(StatementInfo {
            keyword: statement.word(0).to_string(),
            risk,
            reason,
            preview: preview(&statement.text),
            recognized,
        });
    }

    let risk = statements
        .iter()
        .map(|s| s.risk)
        .max()
        .unwrap_or(Risk::Read);

    let mut ordered: Vec<&StatementInfo> = statements.iter().collect();
    ordered.sort_by_key(|statement| std::cmp::Reverse(statement.risk));
    let mut reasons: Vec<String> = Vec::new();
    for statement in ordered {
        if let Some(reason) = &statement.reason
            && !reasons.contains(reason)
        {
            reasons.push(reason.clone());
        }
    }

    let returns_rows = parsed.last().is_some_and(|last| {
        matches!(
            last.word(0),
            "SELECT" | "VALUES" | "TABLE" | "SHOW" | "WITH" | "EXPLAIN" | "PRAGMA"
        ) || last.has("RETURNING")
    });

    SqlAnalysis {
        statements,
        risk,
        reasons,
        returns_rows,
        unbalanced_transaction: depth > 0,
    }
}

/// Refuses to continue unless the caller has acknowledged the risk.
///
/// Destructive SQL always needs `Some(Risk::Destructive)`. Plain writes need
/// an acknowledgement only when `confirm_writes` is on.
pub fn require_acknowledgement(
    analysis: &SqlAnalysis,
    acknowledged: Option<Risk>,
    confirm_writes: bool,
) -> Result<()> {
    let needed = match analysis.risk {
        Risk::Destructive => Some(Risk::Destructive),
        Risk::Write if confirm_writes => Some(Risk::Write),
        _ => None,
    };

    match needed {
        Some(needed) if acknowledged < Some(needed) => {
            let message = match needed {
                Risk::Destructive => {
                    "This statement can remove or overwrite data. Confirm to run it."
                }
                _ => "This statement changes the database. Confirm to run it.",
            };
            Err(KairoError::new(ErrorKind::ConfirmationRequired, message)
                .with_detail(analysis.reasons.join(" ")))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risk(sql: &str) -> Risk {
        analyze(sql).risk
    }

    #[test]
    fn reads_are_reads() {
        for sql in [
            "SELECT * FROM users",
            "select id from users where name = 'DROP TABLE users'",
            "  -- note\n  SELECT 1;",
            "WITH a AS (SELECT 1) SELECT * FROM a",
            "EXPLAIN SELECT * FROM users",
            "EXPLAIN QUERY PLAN SELECT * FROM users",
            "PRAGMA table_info(users)",
            "PRAGMA foreign_keys",
            "VALUES (1), (2)",
            "SHOW server_version",
            "SELECT \"delete\", `update` FROM t",
        ] {
            assert_eq!(risk(sql), Risk::Read, "{sql}");
        }
    }

    #[test]
    fn additive_statements_are_writes() {
        for sql in [
            "INSERT INTO users (name) VALUES ('a')",
            "CREATE TABLE t (a INT)",
            "CREATE INDEX i ON t (a)",
            "PRAGMA journal_mode = WAL",
            "PRAGMA journal_mode(WAL)",
            "VACUUM",
            "SELECT * INTO backup FROM users",
            "WITH n AS (SELECT 1) INSERT INTO t SELECT * FROM n",
            "SET search_path TO app",
        ] {
            assert_eq!(risk(sql), Risk::Write, "{sql}");
        }
    }

    #[test]
    fn anything_that_can_lose_data_is_destructive() {
        for sql in [
            "DROP TABLE users",
            "drop index i",
            "DELETE FROM users",
            "DELETE FROM users WHERE id = 1",
            "UPDATE users SET name = 'x'",
            "TRUNCATE users",
            "ALTER TABLE users DROP COLUMN name",
            "INSERT OR REPLACE INTO t VALUES (1)",
            "INSERT INTO t VALUES (1) ON CONFLICT (id) DO UPDATE SET a = 1",
            "REPLACE INTO t VALUES (1)",
            "CREATE OR REPLACE VIEW v AS SELECT 1",
            "WITH gone AS (DELETE FROM t RETURNING *) SELECT * FROM gone",
            "EXPLAIN ANALYZE DELETE FROM t",
            "GRANT ALL ON t TO bob",
            "CALL purge_everything()",
            "DO $$ BEGIN DELETE FROM t; END $$",
        ] {
            assert_eq!(risk(sql), Risk::Destructive, "{sql}");
        }
    }

    #[test]
    fn unknown_statements_are_gated_but_not_called_destructive_by_name() {
        // A typo must not run unchecked, and must not be described as a
        // statement that is known to destroy data.
        let typo = analyze("SELEC * FROM users");
        assert_eq!(typo.risk, Risk::Destructive);
        assert!(!typo.statements[0].recognized);
        assert_eq!(
            typo.reasons,
            ["Kairo does not recognise SELEC, so it asks before running it."]
        );

        let known = analyze("DROP TABLE users; SELECT 1; INSERT INTO t VALUES (1)");
        assert!(known.statements.iter().all(|s| s.recognized));
    }

    #[test]
    fn the_worst_statement_in_a_script_decides() {
        let analysis = analyze("SELECT 1; INSERT INTO t VALUES (1); DROP TABLE t;");
        assert_eq!(analysis.statements.len(), 3);
        assert_eq!(analysis.risk, Risk::Destructive);
        assert!(analysis.reasons[0].contains("DROP permanently removes the table"));
        assert!(!analysis.returns_rows);
    }

    #[test]
    fn missing_where_is_called_out() {
        let all = analyze("DELETE FROM users");
        assert!(all.reasons[0].contains("every row"), "{:?}", all.reasons);
        let some = analyze("DELETE FROM users WHERE id = 1");
        assert!(!some.reasons[0].contains("every row"));
        // A WHERE inside a string or comment does not count.
        let fake = analyze("UPDATE t SET note = 'WHERE' -- WHERE id = 1");
        assert!(fake.reasons[0].contains("every row"), "{:?}", fake.reasons);
    }

    #[test]
    fn hidden_statements_are_found() {
        // The semicolon and DROP sit after a string that contains a quote.
        assert_eq!(risk("SELECT 'it''s'; DROP TABLE t"), Risk::Destructive);
        // A backslash-escaped quote in an E'' string does not end it early.
        assert_eq!(risk("SELECT E'a\\'b'; DROP TABLE t"), Risk::Destructive);
        // Statements inside comments and strings are not statements.
        assert_eq!(risk("SELECT 1 /* ; DROP TABLE t */"), Risk::Read);
        assert_eq!(risk("SELECT '; DROP TABLE t'"), Risk::Read);
        assert_eq!(risk("SELECT $q$; DROP TABLE t$q$"), Risk::Read);
        assert_eq!(
            risk("/* a /* nested */ still comment */ SELECT 1"),
            Risk::Read
        );
    }

    #[test]
    fn dollar_parameters_are_not_dollar_quotes() {
        let analysis = analyze("SELECT * FROM t WHERE a = $1 AND b = $2; DROP TABLE t");
        assert_eq!(analysis.statements.len(), 2);
        assert_eq!(analysis.risk, Risk::Destructive);
    }

    #[test]
    fn returns_rows_follows_the_last_statement() {
        assert!(analyze("SELECT 1").returns_rows);
        assert!(analyze("INSERT INTO t VALUES (1) RETURNING id").returns_rows);
        assert!(!analyze("INSERT INTO t VALUES (1)").returns_rows);
        assert!(analyze("CREATE TABLE t (a INT); SELECT * FROM t").returns_rows);
    }

    #[test]
    fn open_transactions_are_detected() {
        assert!(analyze("BEGIN; UPDATE t SET a = 1 WHERE id = 1").unbalanced_transaction);
        assert!(!analyze("BEGIN; UPDATE t SET a = 1 WHERE id = 1; COMMIT").unbalanced_transaction);
        assert!(!analyze("BEGIN; SAVEPOINT s; ROLLBACK TO s; ROLLBACK").unbalanced_transaction);
        assert!(analyze("BEGIN; SAVEPOINT s; ROLLBACK TO s").unbalanced_transaction);
        assert!(!analyze("SELECT CASE WHEN 1 THEN 2 END").unbalanced_transaction);
    }

    #[test]
    fn empty_input_has_no_statements() {
        let analysis = analyze("  -- only a comment\n ; ; ");
        assert!(analysis.statements.is_empty());
        assert_eq!(analysis.risk, Risk::Read);
    }

    #[test]
    fn acknowledgement_is_required_at_the_right_level() {
        let read = analyze("SELECT 1");
        let write = analyze("INSERT INTO t VALUES (1)");
        let destructive = analyze("DROP TABLE t");

        assert!(require_acknowledgement(&read, None, true).is_ok());

        assert!(require_acknowledgement(&write, None, false).is_ok());
        let err = require_acknowledgement(&write, None, true).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ConfirmationRequired);
        assert!(require_acknowledgement(&write, Some(Risk::Write), true).is_ok());

        // Destructive SQL cannot be waved through by a setting or a lower
        // acknowledgement.
        assert!(require_acknowledgement(&destructive, None, false).is_err());
        assert!(require_acknowledgement(&destructive, Some(Risk::Write), false).is_err());
        assert!(require_acknowledgement(&destructive, Some(Risk::Destructive), false).is_ok());
    }
}
