use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub environment: Environment,
    pub federation_sync_enabled: bool,
    pub dag_anchoring_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Environment {
    #[serde(rename = "development")]
    Development,
    #[serde(rename = "production")]
    Production,
    #[serde(rename = "test")]
    Test,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/agoranet".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            environment: match std::env::var("ENVIRONMENT").as_deref() {
                Ok("production") => Environment::Production,
                Ok("test") => Environment::Test,
                _ => Environment::Development,
            },
            federation_sync_enabled: std::env::var("FEDERATION_SYNC_ENABLED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
            dag_anchoring_enabled: std::env::var("DAG_ANCHORING_ENABLED")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(true),
        }
    }
} 