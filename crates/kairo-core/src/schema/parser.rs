//! Parses `.kairo` text into a [`Schema`] and reports problems with positions.

use super::{DefaultValue, Field, KNOWN_TYPES, Schema, Table};
use crate::error::{ErrorKind, KairoError, Result};
use pest::Parser;
use pest::error::{ErrorVariant, LineColLocation};
use pest::iterators::Pair;
use pest_derive::Parser;
use serde::Serialize;

#[derive(Parser)]
#[grammar = "schema/kairo.pest"]
struct KairoParser;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// A problem at a 1-based line and column. The end position is exclusive.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    /// No errors. Warnings do not make a schema invalid.
    pub valid: bool,
    /// Present whenever the text parsed, even if it has semantic errors.
    pub schema: Option<Schema>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses a schema, returning the first error if there is one.
pub fn parse_schema(input: &str) -> Result<Schema> {
    let report = validate(input);
    if let Some(first) = report
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
    {
        return Err(KairoError::new(
            ErrorKind::Schema,
            format!(
                "schema error at line {}, column {}: {}",
                first.line, first.column, first.message
            ),
        ));
    }
    report
        .schema
        .ok_or_else(|| KairoError::internal("schema validated but produced no model"))
}

/// Parses and checks a schema. Never fails: problems come back as diagnostics.
pub fn validate(input: &str) -> ValidationReport {
    // Editors on Windows often prepend a byte order mark.
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);

    let mut pairs = match KairoParser::parse(Rule::schema, input) {
        Ok(pairs) => pairs,
        Err(err) => {
            return ValidationReport {
                valid: false,
                schema: None,
                diagnostics: vec![syntax_diagnostic(input, &err)],
            };
        }
    };

    let mut schema = Schema::default();
    let mut diagnostics = Vec::new();

    if let Some(root) = pairs.next() {
        for pair in root.into_inner() {
            if pair.as_rule() == Rule::table {
                schema.tables.push(build_table(pair, &mut diagnostics));
            }
        }
    }

    check_duplicate_tables(&schema, &mut diagnostics);

    if schema.tables.is_empty() {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message: "No tables are defined yet. Start with `table name { field: type }`."
                .to_string(),
            line: 1,
            column: 1,
            end_line: 1,
            end_column: 2,
        });
    }

    diagnostics.sort_by_key(|d| (d.line, d.column));
    let valid = !diagnostics.iter().any(|d| d.severity == Severity::Error);

    ValidationReport {
        valid,
        schema: Some(schema),
        diagnostics,
    }
}

fn span_diagnostic(pair: &Pair<'_, Rule>, severity: Severity, message: String) -> Diagnostic {
    let span = pair.as_span();
    let (line, column) = span.start_pos().line_col();
    let (end_line, end_column) = span.end_pos().line_col();
    Diagnostic {
        severity,
        message,
        line: line as u32,
        column: column as u32,
        end_line: end_line as u32,
        end_column: end_column as u32,
    }
}

/// Returns the name and whether it was written in quotes.
fn read_ident(pair: &Pair<'_, Rule>) -> (String, bool) {
    let text = pair.as_str();
    match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        Some(inner) => (inner.to_string(), true),
        None => (text.to_string(), false),
    }
}

fn build_table(pair: Pair<'_, Rule>, diagnostics: &mut Vec<Diagnostic>) -> Table {
    let line = pair.as_span().start_pos().line_col().0 as u32;
    let mut inner = pair.into_inner();

    // The grammar guarantees a table starts with its name.
    let name_pair = inner.next().expect("table has a name");
    let (name, quoted) = read_ident(&name_pair);

    let mut fields: Vec<Field> = Vec::new();
    for field_pair in inner {
        let field = build_field(field_pair.clone(), diagnostics);
        if fields
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(&field.name))
        {
            let name_pair = field_pair.into_inner().next().expect("field has a name");
            diagnostics.push(span_diagnostic(
                &name_pair,
                Severity::Error,
                format!("Field `{}` is defined twice in table `{name}`.", field.name),
            ));
        }
        fields.push(field);
    }

    if fields.is_empty() {
        diagnostics.push(span_diagnostic(
            &name_pair,
            Severity::Error,
            format!("Table `{name}` has no fields. Add at least one, like `id: int`."),
        ));
    }

    Table {
        name,
        quoted,
        fields,
        line,
    }
}

fn build_field(pair: Pair<'_, Rule>, diagnostics: &mut Vec<Diagnostic>) -> Field {
    let line = pair.as_span().start_pos().line_col().0 as u32;
    let mut field = Field {
        line,
        ..Field::default()
    };

    for part in pair.into_inner() {
        match part.as_rule() {
            Rule::ident => {
                let (name, quoted) = read_ident(&part);
                field.name = name;
                field.quoted = quoted;
            }
            Rule::type_name => {
                field.type_name = part.as_str().to_string();
                if !field.is_known_type() {
                    diagnostics.push(span_diagnostic(
                        &part,
                        Severity::Warning,
                        format!(
                            "Unknown type `{}` is stored as text. Known types: {}.",
                            field.type_name,
                            KNOWN_TYPES.join(", ")
                        ),
                    ));
                }
            }
            Rule::default_value => {
                let literal = part
                    .clone()
                    .into_inner()
                    .next()
                    .expect("default has a literal");
                let value = match literal.as_rule() {
                    Rule::bool_lit => DefaultValue::Bool(literal.as_str() == "true"),
                    Rule::float_lit => DefaultValue::Float(literal.as_str().to_string()),
                    Rule::int_lit => DefaultValue::Int(literal.as_str().to_string()),
                    _ => {
                        let text = literal.as_str();
                        DefaultValue::Text(text[1..text.len() - 1].to_string())
                    }
                };
                if let Some(message) = default_mismatch(&field.type_name, &value) {
                    diagnostics.push(span_diagnostic(&part, Severity::Warning, message));
                }
                field.default_value = Some(value);
            }
            Rule::modifiers => {
                for modifier in part.into_inner() {
                    let flag = match modifier.as_str() {
                        "required" => &mut field.required,
                        "primary" => &mut field.primary,
                        _ => &mut field.unique,
                    };
                    if *flag {
                        diagnostics.push(span_diagnostic(
                            &modifier,
                            Severity::Warning,
                            format!("`{}` is listed twice.", modifier.as_str()),
                        ));
                    }
                    *flag = true;
                }
            }
            _ => {}
        }
    }

    field
}

/// Explains a default whose literal does not suit the field type.
fn default_mismatch(type_name: &str, value: &DefaultValue) -> Option<String> {
    let found = match value {
        DefaultValue::Bool(_) => "true or false",
        DefaultValue::Int(_) => "a whole number",
        DefaultValue::Float(_) => "a decimal number",
        DefaultValue::Text(_) => "text",
    };
    let fits = match (type_name, value) {
        ("bool", DefaultValue::Bool(_)) => true,
        ("int", DefaultValue::Int(_)) => true,
        ("float", DefaultValue::Int(_) | DefaultValue::Float(_)) => true,
        ("string" | "timestamp", DefaultValue::Text(_)) => true,
        ("bool" | "int" | "float" | "string" | "timestamp" | "blob", _) => false,
        // Unknown types are stored as text and already carry a warning.
        _ => true,
    };
    (!fits).then(|| format!("The default is {found}, but the field type is `{type_name}`."))
}

fn check_duplicate_tables(schema: &Schema, diagnostics: &mut Vec<Diagnostic>) {
    for (index, table) in schema.tables.iter().enumerate() {
        let repeated = schema.tables[..index]
            .iter()
            .any(|earlier| earlier.name.eq_ignore_ascii_case(&table.name));
        if repeated {
            let column = "table ".len() as u32 + 1;
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: format!("Table `{}` is defined twice.", table.name),
                line: table.line,
                column,
                end_line: table.line,
                end_column: column + table.name.chars().count() as u32,
            });
        }
    }
}

fn friendly_rule(rule: Rule) -> &'static str {
    match rule {
        Rule::table => "a table (`table name { … }`)",
        Rule::field => "a field (`name: type`)",
        Rule::ident | Rule::bare_ident | Rule::quoted_ident => "a name",
        Rule::type_name => "a type",
        Rule::default_value
        | Rule::bool_lit
        | Rule::int_lit
        | Rule::float_lit
        | Rule::string_lit => "a default value",
        Rule::modifiers => "`[`",
        Rule::modifier => "`required`, `primary` or `unique`",
        Rule::EOI => "the end of the file",
        _ => "more input",
    }
}

/// Turns a pest failure into a message that says what to change.
fn syntax_diagnostic(input: &str, err: &pest::error::Error<Rule>) -> Diagnostic {
    let (line, column) = match err.line_col {
        LineColLocation::Pos(pos) => pos,
        LineColLocation::Span(start, _) => start,
    };

    let source_line = input.lines().nth(line - 1).unwrap_or("");
    // Columns count characters; slice by character, not by byte.
    let rest: String = source_line.chars().skip(column - 1).collect();
    let rest = rest.split("//").next().unwrap_or("").trim_end().to_string();
    let at_end = rest.trim().is_empty() && input.lines().skip(line).all(is_blank_or_comment);

    let positives: Vec<Rule> = match &err.variant {
        ErrorVariant::ParsingError { positives, .. } => positives.clone(),
        ErrorVariant::CustomError { .. } => Vec::new(),
    };
    let expects = |rule: Rule| positives.contains(&rule);

    // pest reports the rule that failed furthest into the input. When every
    // alternative fails where a rule began, it reports that enclosing rule,
    // which is why `schema` can stand in for `table` here.
    let message = if at_end && unclosed_braces(input) > 0 {
        "This table is never closed. Add `}` at the end.".to_string()
    } else if expects(Rule::default_value) {
        DEFAULT_HELP.to_string()
    } else if expects(Rule::modifier) {
        MODIFIER_HELP.to_string()
    } else if expects(Rule::type_name) {
        format!(
            "Expected a type after `:`. Known types: {}.",
            KNOWN_TYPES.join(", ")
        )
    } else if expects(Rule::field) && !rest.is_empty() && !rest.starts_with('}') {
        explain_field(&rest)
    } else if expects(Rule::table) || expects(Rule::schema) {
        match rest.split_whitespace().next() {
            Some(word) => format!(
                "Expected `table`, found `{word}`. Tables look like `table name {{ field: type }}`."
            ),
            None => "Expected a table: `table name { field: type }`.".to_string(),
        }
    } else if expects(Rule::ident) || expects(Rule::bare_ident) {
        "Expected a name here. Names use letters, digits and `_`, or are written in \"quotes\"."
            .to_string()
    } else if positives.is_empty() {
        "This is not valid .kairo syntax.".to_string()
    } else {
        let mut names: Vec<&str> = positives.iter().map(|r| friendly_rule(*r)).collect();
        names.dedup();
        format!("Expected {}.", names.join(" or "))
    };

    let width = rest.chars().count().max(1) as u32;
    Diagnostic {
        severity: Severity::Error,
        message,
        line: line as u32,
        column: column as u32,
        end_line: line as u32,
        end_column: column as u32 + width,
    }
}

fn is_blank_or_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with("//")
}

/// Counts `{` that are never matched by `}`, ignoring comments and strings.
fn unclosed_braces(input: &str) -> i32 {
    let mut depth = 0;
    for line in input.lines() {
        let mut in_string = false;
        let mut previous = '\0';
        for ch in line.chars() {
            match ch {
                '"' => in_string = !in_string,
                '/' if previous == '/' && !in_string => break,
                '{' if !in_string => depth += 1,
                '}' if !in_string => depth -= 1,
                _ => {}
            }
            previous = ch;
        }
    }
    depth
}

/// Explains what is wrong with a field, given the text from its first
/// character to the end of the line.
fn explain_field(rest: &str) -> String {
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(name_len);
    let after = after.trim_start();

    if name.is_empty() && !rest.starts_with('"') {
        return format!(
            "Expected a field name, found `{}`.",
            rest.chars().next().unwrap_or(' ')
        );
    }
    let name = if name.is_empty() { "the field" } else { name };

    let Some(after_colon) = after.strip_prefix(':') else {
        return format!("Expected `:` after `{name}`. Write fields as `{name}: type`.");
    };
    let after_colon = after_colon.trim_start();
    let type_len = after_colon
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_colon.len());
    if type_len == 0 {
        return format!(
            "Expected a type after `{name}:`. Known types: {}.",
            KNOWN_TYPES.join(", ")
        );
    }

    let tail = after_colon[type_len..].trim_start();
    if tail.starts_with('=') {
        return DEFAULT_HELP.to_string();
    }
    if tail.starts_with('[') {
        return MODIFIER_HELP.to_string();
    }
    format!("Unexpected text after the type of `{name}`. Put each field on its own line.")
}

const DEFAULT_HELP: &str = "Default values must be `true`, `false`, a number, or \"quoted text\".";
const MODIFIER_HELP: &str = "Modifiers are `required`, `primary` and `unique`, inside one pair \
                             of brackets, like `[required, unique]`.";

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(input: &str) -> Vec<Diagnostic> {
        validate(input)
            .diagnostics
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect()
    }

    #[test]
    fn parses_a_0_4_schema_unchanged() {
        // The sample schema shipped with 0.4.0, on one line with commas.
        let schema = parse_schema("table users { name: string, age: int }").unwrap();
        assert_eq!(schema.tables.len(), 1);
        let users = &schema.tables[0];
        assert_eq!(users.name, "users");
        assert_eq!(users.fields.len(), 2);
        assert_eq!(users.fields[0].name, "name");
        assert_eq!(users.fields[0].type_name, "string");
        assert_eq!(users.fields[1].type_name, "int");
        assert!(!users.fields[0].required && !users.fields[0].primary);
    }

    #[test]
    fn parses_defaults_comments_and_several_tables() {
        let schema = parse_schema(
            "// people\n\
             table users {\n  name: string\n  active: bool = true // flag\n  views: int = -3\n}\n\
             table posts { title: string = \"untitled\" }\n",
        )
        .unwrap();
        assert_eq!(schema.tables.len(), 2);
        assert_eq!(
            schema.tables[0].fields[1].default_value,
            Some(DefaultValue::Bool(true))
        );
        assert_eq!(
            schema.tables[0].fields[2].default_value,
            Some(DefaultValue::Int("-3".into()))
        );
        assert_eq!(
            schema.tables[1].fields[0].default_value,
            Some(DefaultValue::Text("untitled".into()))
        );
        assert_eq!(schema.tables[1].line, 7);
    }

    #[test]
    fn parses_modifiers_floats_and_quoted_names() {
        let schema = parse_schema(
            "table \"Order Items\" {\n\
               id: int [primary]\n\
               sku: string [required, unique]\n\
               price: float = 9.99 [required]\n\
               \"unit price\": float\n\
             }",
        )
        .unwrap();
        let table = &schema.tables[0];
        assert_eq!(table.name, "Order Items");
        assert!(table.quoted);
        assert!(table.fields[0].primary);
        assert!(table.fields[1].required && table.fields[1].unique);
        assert_eq!(
            table.fields[2].default_value,
            Some(DefaultValue::Float("9.99".into()))
        );
        assert!(table.fields[2].required);
        assert_eq!(table.fields[3].name, "unit price");
        assert!(table.fields[3].quoted);
    }

    #[test]
    fn strips_a_byte_order_mark() {
        assert!(parse_schema("\u{FEFF}table t { a: int }").is_ok());
    }

    #[test]
    fn reports_a_missing_colon_at_the_field() {
        let found = errors("table users {\n  name string\n}");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].column), (2, 3));
        assert!(
            found[0].message.contains("Expected `:` after `name`"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn reports_a_missing_type() {
        let found = errors("table users {\n  name:\n}");
        assert!(
            found[0].message.contains("Expected a type"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn reports_an_unclosed_table() {
        let found = errors("table users {\n  name: string\n");
        assert!(
            found[0].message.contains("never closed"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn reports_a_misspelled_keyword() {
        let found = errors("tabel users { name: string }");
        assert_eq!((found[0].line, found[0].column), (1, 1));
        assert!(
            found[0].message.contains("found `tabel`"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn reports_bad_defaults_and_modifiers() {
        let found = errors("table t {\n  a: int = nope\n}");
        assert!(
            found[0].message.contains("Default values"),
            "{}",
            found[0].message
        );
        let found = errors("table t {\n  a: int [optional]\n}");
        assert!(
            found[0].message.contains("Modifiers are"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn reports_duplicates_and_empty_tables() {
        let found = errors("table t { a: int, A: string }\ntable T { b: int }\ntable e { }");
        let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Field `A` is defined twice"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Table `T` is defined twice"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Table `e` has no fields"))
        );
    }

    #[test]
    fn unknown_types_and_mismatched_defaults_only_warn() {
        let report = validate("table t {\n  a: money\n  b: int = \"x\"\n}");
        assert!(report.valid);
        assert_eq!(report.diagnostics.len(), 2);
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning)
        );
        assert_eq!(report.diagnostics[0].line, 2);
    }

    #[test]
    fn parse_schema_surfaces_the_first_error_with_its_position() {
        let err = parse_schema("table users {\n  name string\n}").unwrap_err();
        assert_eq!(err.kind, ErrorKind::Schema);
        assert!(
            err.message.starts_with("schema error at line 2, column 3"),
            "{}",
            err.message
        );
    }

    #[test]
    fn an_empty_file_is_valid_but_warns() {
        let report = validate("// nothing yet\n");
        assert!(report.valid);
        assert_eq!(report.diagnostics.len(), 1);
    }
}
