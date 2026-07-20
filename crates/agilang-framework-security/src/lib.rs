use anyhow::{bail, Result};

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut res = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        res |= x ^ y;
    }
    res == 0
}

pub struct CsrfTokenManager;

impl CsrfTokenManager {
    pub fn generate_field(token: &str) -> String {
        format!(
            "<input type=\"hidden\" name=\"_token\" value=\"{}\">",
            token
        )
    }

    pub fn verify(
        request_method: &str,
        session_token: &str,
        submitted_token: Option<&str>,
    ) -> Result<()> {
        let method = request_method.to_uppercase();
        if method == "GET" || method == "HEAD" || method == "OPTIONS" {
            return Ok(());
        }

        let submitted = match submitted_token {
            Some(t) => t,
            None => bail!("419: CSRF token missing"),
        };

        if constant_time_eq(session_token.as_bytes(), submitted.as_bytes()) {
            Ok(())
        } else {
            bail!("419: CSRF token mismatch")
        }
    }
}

pub struct SecureHeaders;

impl SecureHeaders {
    pub fn apply(headers: &mut Vec<(String, String)>, is_https: bool) {
        headers.push(("X-Content-Type-Options".to_string(), "nosniff".to_string()));
        headers.push(("X-Frame-Options".to_string(), "SAMEORIGIN".to_string()));
        headers.push(("X-XSS-Protection".to_string(), "1; mode=block".to_string()));

        if is_https {
            headers.push((
                "Strict-Transport-Security".to_string(),
                "max-age=31536000; includeSubDomains".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csrf_verification() {
        let session_token = "secret_session_csrf_token_123456";

        assert!(CsrfTokenManager::verify("GET", session_token, None).is_ok());

        assert!(CsrfTokenManager::verify("POST", session_token, Some(session_token)).is_ok());

        let err = CsrfTokenManager::verify("POST", session_token, Some("invalid_token"));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("419"));
    }

    #[test]
    fn test_csrf_field_generation() {
        let token = "token123";
        let field = CsrfTokenManager::generate_field(token);
        assert_eq!(
            field,
            "<input type=\"hidden\" name=\"_token\" value=\"token123\">"
        );
    }
}
