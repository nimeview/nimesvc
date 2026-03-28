use crate::types::Token;

pub fn login(username: String, password: String) -> Result<Token, String> {
    if !username.is_empty() && password == "secret" {
        Ok(Token {
            token: format!("tok_{}", username),
        })
    } else {
        Err("unauthorized".to_string())
    }
}

pub fn validate(token: String) -> bool {
    token.starts_with("tok_")
}
