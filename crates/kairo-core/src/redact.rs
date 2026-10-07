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
    fn does_not_touch_unrelated_words() {
        assert_eq!(
            redact("the forgot_password=flow"),
            "the forgot_password=flow"
        );
        assert_eq!(redact("no secrets here"), "no secrets here");
        assert_eq!(redact("naïve café ünïcode"), "naïve café ünïcode");
    }
}
