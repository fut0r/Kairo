//! Credential masking.
//!
//! Anything that may be shown, logged, stored or exported goes through
//! [`redact`]. It errs on the side of masking too much.

/// What a masked secret is replaced with.
pub const MASK: &str = "••••";

/// Keys whose values are secrets in `key=value` settings and URL queries.
const SECRET_KEYS: [&str; 5] = ["password", "sslpassword", "passwd", "pwd", "passfile"];

/// Masks passwords in connection URLs and in `key=value` settings found
/// anywhere inside `input`.
pub fn redact(input: &str) -> String {
    redact_key_values(&redact_urls(input))
}

/// Masks secrets in SQL before it is kept in history: everything [`redact`]
/// covers, plus the literal in `PASSWORD '…'`, as in `ALTER ROLE … PASSWORD`.
pub fn redact_sql(sql: &str) -> String {
    let input = redact(sql);
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut copied = 0;
    let mut search = 0;

    while let Some(found) = lower[search..].find("password") {
        let start = search + found;
        let after = start + "password".len();
        search = after;

        let standalone = !input[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        // Accept `PASSWORD 'x'`, `password = 'x'` and `password='x'`.
        let rest = &input[after..];
        let spaced = rest.trim_start();
        let assigned = spaced.strip_prefix('=').map(str::trim_start);
        let separated = spaced.len() < rest.len() || assigned.is_some();
        let literal = assigned.unwrap_or(spaced);
        let literal_at = input.len() - literal.len();
        if !standalone || !separated || !literal.starts_with('\'') {
            continue;
        }

        // Find the closing quote; a doubled quote is an escaped one.
        let body = &input[literal_at + 1..];
        let mut end = body.len();
        let mut chars = body.char_indices().peekable();
        while let Some((at, ch)) = chars.next() {
            if ch == '\'' {
                if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                    chars.next();
                } else {
                    end = at;
                    break;
                }
            }
        }

        out.push_str(&input[copied..=literal_at]);
        out.push_str(MASK);
        copied = literal_at + 1 + end;
        search = copied;
    }

    out.push_str(&input[copied..]);
    out
}

/// Masks the password of every `scheme://user:password@host` in `input`.
fn redact_urls(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(pos) = rest.find("://") {
        let after = pos + 3;
        out.push_str(&rest[..after]);
        let tail = &rest[after..];

        // The URL runs until whitespace or a quote.
        let token_end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`'))
            .unwrap_or(tail.len());
        let token = &tail[..token_end];

        // Prefer the last `@` inside the authority. If the password itself
        // holds a `/`, fall back to the last `@` in the whole token: masking
        // too much is acceptable, leaking is not.
        let authority_end = token.find(['/', '?', '#']).unwrap_or(token.len());
        let at = token[..authority_end]
            .rfind('@')
            .or_else(|| token.rfind('@'));

        match at {
            Some(at) => {
                let userinfo = &token[..at];
                match userinfo.find(':') {
                    Some(colon) => {
                        out.push_str(&userinfo[..=colon]);
                        out.push_str(MASK);
                    }
                    None => out.push_str(userinfo),
                }
                out.push_str(&token[at..]);
            }
            None => out.push_str(token),
        }

        rest = &tail[token_end..];
    }

    out.push_str(rest);
    out
}

/// Masks the value of `password=…` style settings.
fn redact_key_values(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        let matched = SECRET_KEYS.iter().find(|key| {
            lower[i..].starts_with(**key)
                && bytes.get(i + key.len()) == Some(&b'=')
                && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        });

        let Some(key) = matched else {
            // Copy one whole character so multi-byte text stays intact.
            let ch = input[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        };

        let value_start = i + key.len() + 1;
        out.push_str(&input[i..value_start]);

        let value = &input[value_start..];
        let value_len = match value.chars().next() {
            Some(quote @ ('\'' | '"')) => value[1..]
                .find(quote)
                .map(|end| end + 2)
                .unwrap_or(value.len()),
            _ => value
                .find(|c: char| c.is_whitespace() || matches!(c, '&' | ';' | ')' | '"' | '\''))
                .unwrap_or(value.len()),
        };

        if value_len > 0 {
            out.push_str(MASK);
        }
        i = value_start + value_len;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_url_password() {
        assert_eq!(
            redact("postgres://alice:s3cret@db.internal:5432/app"),
            format!("postgres://alice:{MASK}@db.internal:5432/app")
        );
    }

    #[test]
    fn leaves_urls_without_password_alone() {
        let url = "postgres://alice@db.internal:5432/app?sslmode=require";
        assert_eq!(redact(url), url);
        assert_eq!(
            redact("https://kairo.arabdev.site/"),
            "https://kairo.arabdev.site/"
        );
    }

    #[test]
    fn masks_inside_a_sentence() {
        let out = redact("failed to connect to postgres://u:p%40ss@h/db: timeout");
        assert!(!out.contains("p%40ss"));
        assert!(out.ends_with("@h/db: timeout"));
    }

    #[test]
    fn masks_password_containing_slash_or_at() {
        let out = redact("postgresql://bob:pa/ss@word@host/db");
        assert!(!out.contains("pa/ss"), "{out}");
        assert!(!out.contains("word@"), "{out}");
    }

    #[test]
    fn masks_every_url_in_the_text() {
        let out = redact("a postgres://u:one@h/x b postgres://u:two@h/y");
        assert!(!out.contains("one") && !out.contains("two"), "{out}");
    }

    #[test]
    fn masks_key_value_settings() {
        let out = redact("host=db user=alice password=s3cret dbname=app");
        assert_eq!(
            out,
            format!("host=db user=alice password={MASK} dbname=app")
        );

        let out = redact("postgres://h/db?sslmode=require&password=s3cret&x=1");
        assert_eq!(
            out,
            format!("postgres://h/db?sslmode=require&password={MASK}&x=1")
        );

        let out = redact("PASSWORD='two words' next");
        assert_eq!(out, format!("PASSWORD={MASK} next"));
    }

    #[test]
    fn sql_password_literals_are_masked() {
        assert_eq!(
            redact_sql("ALTER ROLE app WITH PASSWORD 'hunter2' VALID UNTIL 'infinity'"),
            format!("ALTER ROLE app WITH PASSWORD '{MASK}' VALID UNTIL 'infinity'")
        );
        assert_eq!(
            redact_sql("create user x password   'it''s; secret'; select 1"),
            format!("create user x password   '{MASK}'; select 1")
        );
        // An unterminated literal is masked to the end rather than leaked.
        assert_eq!(redact_sql("PASSWORD 'oops"), format!("PASSWORD '{MASK}"));
        // A value being written to a column called password is a secret too.
        assert_eq!(
            redact_sql("UPDATE users SET password = 'abc' WHERE id = 1"),
            format!("UPDATE users SET password = '{MASK}' WHERE id = 1")
        );
    }

    #[test]
    fn sql_without_secrets_is_untouched() {
        for sql in [
            "SELECT password FROM users WHERE name = 'bob'",
            "SELECT * FROM password_resets",
            "UPDATE users SET password_hash = 'abc' WHERE id = 1",
            "SELECT 'naïve' AS café",
        ] {
            assert_eq!(redact_sql(sql), sql);
        }
    }

    #[test]
    fn does_not_touch_unrelated_words() {
        assert_eq!(
            redact("the forgot_password=flow"),
            "the forgot_password=flow"
        );
        assert_eq!(redact("no secrets here"), "no secrets here");
        assert_eq!(redact("naïve café ünïcode"), "naïve café ünïcode");
    }
}
