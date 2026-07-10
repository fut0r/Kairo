pub fn translate_query(query: &str) -> String {
    let trimmed = query.trim();
    let lowered = trimmed.to_lowercase();

    if lowered.starts_with("from ") {
        format!("SELECT * {}", trimmed)
    } else if lowered.starts_with("show ") || lowered.starts_with("list ") {
        format!("SELECT * FROM {}", trimmed.split_whitespace().skip(1).collect::<Vec<_>>().join(" "))
    } else if lowered.starts_with("count ") && lowered.contains(" from ") {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4 {
            format!("SELECT COUNT(*) FROM {}", parts[parts.len() - 1])
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::translate_query;

    #[test]
    fn translates_natural_queries() {
        assert_eq!(translate_query("from users"), "SELECT * from users");
        assert_eq!(translate_query("show users"), "SELECT * FROM users");
    }
}
