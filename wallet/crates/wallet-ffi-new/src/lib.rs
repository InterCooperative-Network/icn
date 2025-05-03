use thiserror::Error;

#[derive(Debug, Error)]
pub enum MinimalError {
    #[error("An unknown error occurred")]
    Unknown,
}

#[derive(Debug, Clone)]
pub enum Status {
    Ready,
    Busy,
    Error,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub path: String,
}

uniffi::include_scaffolding!("minimal");

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
