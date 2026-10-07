//! Small SQL-building helpers shared by both engines.

/// Quotes an identifier for use in SQL. Works for SQLite and PostgreSQL.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// The character that escapes `%` and `_` in the patterns built here. A
/// backslash would depend on server settings; `!` does not.
pub const LIKE_ESCAPE: char = '!';

/// Builds a `LIKE` pattern that finds `needle` anywhere, as literal text.
pub fn contains_pattern(needle: &str) -> String {
    let mut pattern = String::with_capacity(needle.len() + 2);
    pattern.push('%');
    for ch in needle.chars() {
        if matches!(ch, '%' | '_') || ch == LIKE_ESCAPE {
            pattern.push(LIKE_ESCAPE);
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

/// Maps a declared SQLite column type to the closest `.kairo` type, following
/// SQLite's own affinity rules.
pub fn kairo_type_for_sqlite(declared: &str) -> &'static str {
    let upper = declared.to_ascii_uppercase();
    let has = |needle: &str| upper.contains(needle);

    if has("BOOL") {
        "bool"
    } else if has("INT") {
        "int"
    } else if has("CHAR") || has("CLOB") || has("TEXT") {
        "string"
    } else if has("BLOB") || upper.trim().is_empty() {
        "blob"
    } else if has("REAL") || has("FLOA") || has("DOUB") || has("DEC") || has("NUMERIC") {
        "float"
    } else if has("DATE") || has("TIME") {
        "timestamp"
    } else {
        "string"
    }
}

/// Maps a PostgreSQL type, as `format_type` prints it, to a `.kairo` type.
pub fn kairo_type_for_postgres(data_type: &str) -> &'static str {
    let lower = data_type.to_ascii_lowercase();
    // Arrays have no `.kairo` equivalent; they are shown as text.
    if lower.ends_with("[]") {
        return "string";
    }
    let base = lower.split('(').next().unwrap_or("").trim();

    match base {
        "smallint" | "integer" | "bigint" | "smallserial" | "serial" | "bigserial" | "oid" => "int",
        "boolean" => "bool",
        "real" | "double precision" | "numeric" | "decimal" | "money" => "float",
        "bytea" => "blob",
        "date" => "timestamp",
        _ if base.starts_with("timestamp") => "timestamp",
        _ => "string",
    }
}

/// True when a native type maps onto its `.kairo` type without losing
/// anything worth a note in an exported schema.
pub fn is_canonical_type(declared: &str, kairo_type: &str) -> bool {
    let lower = declared.to_ascii_lowercase();
    let lower = lower.trim();
    match kairo_type {
        "string" => matches!(lower, "text" | "varchar" | "character varying" | "string"),
        "int" => matches!(lower, "integer" | "int"),
        "bool" => matches!(lower, "boolean" | "bool"),
        "float" => matches!(lower, "real" | "float" | "double" | "double precision"),
        "blob" => matches!(lower, "blob" | "bytea"),
        "timestamp" => matches!(
            lower,
            "timestamp" | "datetime" | "timestamp without time zone"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_quoted_and_escaped() {
        assert_eq!(quote_ident("users"), "\"users\"");
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(quote_ident("x; DROP TABLE y"), "\"x; DROP TABLE y\"");
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(contains_pattern("abc"), "%abc%");
        assert_eq!(contains_pattern("50%_off!"), "%50!%!_off!!%");
    }

    #[test]
    fn sqlite_types_follow_affinity() {
        assert_eq!(kairo_type_for_sqlite("INTEGER"), "int");
        assert_eq!(kairo_type_for_sqlite("VARCHAR(255)"), "string");
        assert_eq!(kairo_type_for_sqlite("BOOLEAN"), "bool");
        assert_eq!(kairo_type_for_sqlite("DOUBLE PRECISION"), "float");
        assert_eq!(kairo_type_for_sqlite("DECIMAL(10,2)"), "float");
        assert_eq!(kairo_type_for_sqlite("DATETIME"), "timestamp");
        assert_eq!(kairo_type_for_sqlite(""), "blob");
        assert_eq!(kairo_type_for_sqlite("JSON"), "string");
    }

    #[test]
    fn postgres_types_map_by_base_name() {
        assert_eq!(kairo_type_for_postgres("integer"), "int");
        assert_eq!(kairo_type_for_postgres("bigint"), "int");
        assert_eq!(kairo_type_for_postgres("character varying(255)"), "string");
        assert_eq!(kairo_type_for_postgres("numeric(10,2)"), "float");
        assert_eq!(
            kairo_type_for_postgres("timestamp with time zone"),
            "timestamp"
        );
        assert_eq!(kairo_type_for_postgres("uuid"), "string");
        assert_eq!(kairo_type_for_postgres("integer[]"), "string");
        assert_eq!(kairo_type_for_postgres("bytea"), "blob");
    }

    #[test]
    fn canonical_types_need_no_note() {
        assert!(is_canonical_type("TEXT", "string"));
        assert!(is_canonical_type("integer", "int"));
        assert!(!is_canonical_type("varchar(255)", "string"));
        assert!(!is_canonical_type("bigint", "int"));
        assert!(!is_canonical_type("jsonb", "string"));
    }
}
