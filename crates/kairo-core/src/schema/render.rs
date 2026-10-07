//! Writes a [`Schema`] back out as `.kairo` text the parser accepts.

use super::{DefaultValue, Field, Schema, is_bare_ident};

fn kairo_name(name: &str, quoted: bool) -> String {
    if quoted || !is_bare_ident(name) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

fn field_line(field: &Field) -> String {
    let mut line = format!(
        "  {}: {}",
        kairo_name(&field.name, field.quoted),
        field.type_name
    );

    match &field.default_value {
        Some(DefaultValue::Bool(value)) => line.push_str(&format!(" = {value}")),
        Some(DefaultValue::Int(text) | DefaultValue::Float(text)) => {
            line.push_str(&format!(" = {text}"))
        }
        Some(DefaultValue::Text(text)) => line.push_str(&format!(" = \"{text}\"")),
        None => {}
    }

    let modifiers: Vec<&str> = [
        (field.primary, "primary"),
        (field.required, "required"),
        (field.unique, "unique"),
    ]
    .iter()
    .filter_map(|(set, name)| set.then_some(*name))
    .collect();
    if !modifiers.is_empty() {
        line.push_str(&format!(" [{}]", modifiers.join(", ")));
    }

    if let Some(comment) = &field.comment {
        line.push_str(&format!(" // {comment}"));
    }
    line
}

/// Renders tables separated by one blank line, with a trailing newline.
pub fn render_kairo(schema: &Schema) -> String {
    let mut out = String::new();
    for (index, table) in schema.tables.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "table {} {{\n",
            kairo_name(&table.name, table.quoted)
        ));
        for field in &table.fields {
            out.push_str(&field_line(field));
            out.push('\n');
        }
        out.push_str("}\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Dialect, generate_sql, parse_schema};

    #[test]
    fn rendering_then_parsing_gives_the_same_schema() {
        let source = "table users {\n  id: int [primary]\n  email: string [required, unique]\n  \
                      active: bool = true\n  score: float = 1.5 [required]\n  \
                      note: string = \"n/a\"\n}\n\n\
                      table \"Order Items\" {\n  \"unit price\": float\n  qty: int = -1\n}\n";
        let first = parse_schema(source).unwrap();
        let rendered = render_kairo(&first);
        assert_eq!(rendered, source);

        let second = parse_schema(&rendered).unwrap();
        assert_eq!(
            generate_sql(&first, Dialect::Postgres),
            generate_sql(&second, Dialect::Postgres)
        );
    }

    #[test]
    fn comments_survive_as_trailing_notes() {
        let mut schema = parse_schema("table t { a: string }").unwrap();
        schema.tables[0].fields[0].comment = Some("native: varchar(40)".into());
        let rendered = render_kairo(&schema);
        assert!(rendered.contains("  a: string // native: varchar(40)\n"));
        // The note is a comment, so the output still parses.
        assert!(parse_schema(&rendered).is_ok());
    }
}
