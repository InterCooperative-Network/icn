#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("invalid subject ID (must start with 'did:'): {0}")]
    InvalidSubjectId(String),

    #[error("invalid action format (expected 'domain:verb[:subverb]'): {0}")]
    InvalidAction(String),
}
