use pest_derive::Parser;
use pest::Parser;
use anyhow::{Result, anyhow};

#[derive(Parser)]
#[grammar = "core/kairo.pest"]
pub struct KairoParser;

#[derive(Debug)]
pub struct Schema {
    pub tables: Vec<Table>,
}

#[derive(Debug)]
pub struct Table {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub default_value: Option<String>,
    pub required: bool,
    pub primary_key: bool,
    pub unique: bool,
}

pub fn parse_schema(input: &str) -> Result<Schema> {
    let mut pairs = KairoParser::parse(Rule::schema, input)
        .map_err(|e| anyhow!("Parse error: {}", e))?;

    let mut tables = Vec::new();

    if let Some(schema_pair) = pairs.next() {
        for pair in schema_pair.into_inner() {
            if let Rule::table = pair.as_rule() {
                let mut inner = pair.into_inner();
                let name = inner.next().unwrap().as_str().to_string();
                let mut fields = Vec::new();

                for field_pair in inner {
                    let mut field_inner = field_pair.into_inner();
                    let f_name = field_inner.next().unwrap().as_str().to_string();
                    let f_type = field_inner.next().unwrap().as_str().to_string();
                    let mut f_default = None;
                    let mut is_required = false;
                    let mut is_primary_key = false;
                    let mut is_unique = false;

                    for option_pair in field_inner {
                        match option_pair.as_rule() {
                            Rule::modifier => {
                                let text = option_pair.as_str().trim();
                                match text {
                                    "required" => is_required = true,
                                    "primary" => is_primary_key = true,
                                    "unique" => is_unique = true,
                                    _ => {}
                                }
                            }
                            Rule::default_value => {
                                let inner = option_pair.into_inner().next();
                                if let Some(value) = inner {
                                    f_default = Some(value.as_str().to_string());
                                }
                            }
                            _ => {}
                        }
                    }

                    fields.push(Field {
                        name: f_name,
                        type_name: f_type,
                        default_value: f_default,
                        required: is_required,
                        primary_key: is_primary_key,
                        unique: is_unique,
                    });
                }

                tables.push(Table { name, fields });
            }
        }
    }

    Ok(Schema { tables })
}

#[cfg(test)]
mod tests {
    use super::{Field, Schema, Table, parse_schema};

    #[test]
    fn parses_modifiers_and_defaults() {
        let schema = parse_schema(
            r#"
            table users {
              id: int [primary]
              name: string [required] = "anon"
              email: string [unique]
            }
            "#,
        )
        .expect("schema should parse");

        assert_eq!(schema.tables.len(), 1);
        let user_table = &schema.tables[0];
        assert_eq!(user_table.name, "users");
        assert_eq!(user_table.fields.len(), 3);

        let id_field = &user_table.fields[0];
        assert!(id_field.primary_key);
        assert!(!id_field.required);

        let name_field = &user_table.fields[1];
        assert!(name_field.required);
        assert_eq!(name_field.default_value.as_deref(), Some("\"anon\""));

        let email_field = &user_table.fields[2];
        assert!(email_field.unique);
    }
}
