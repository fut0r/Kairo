use crate::core::parser::Schema;

pub fn generate_sql(schema: &Schema) -> String {
    let mut sql = String::new();

    for table in &schema.tables {
        sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n", table.name));
        
        let mut fields_sql = Vec::new();
        for field in &table.fields {
            let type_sql = match field.type_name.as_str() {
                "string" => "TEXT",
                "int" => "INTEGER",
                "bool" => "BOOLEAN",
                _ => "TEXT",
            };

            let mut field_sql = format!("  {} {}", field.name, type_sql);
            if field.primary_key {
                field_sql.push_str(" PRIMARY KEY");
            }
            if field.required {
                field_sql.push_str(" NOT NULL");
            }
            if field.unique {
                field_sql.push_str(" UNIQUE");
            }
            if let Some(default) = &field.default_value {
                field_sql.push_str(&format!(" DEFAULT {}", default));
            }
            fields_sql.push(field_sql);
        }
        
        sql.push_str(&fields_sql.join(",\n"));
        sql.push_str("\n);\n\n");
    }

    sql
}
