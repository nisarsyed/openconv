use serde::Deserialize;

/// Server configuration loaded from config.toml with env var overrides.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to. Default: "127.0.0.1"
    #[serde(default = "default_host")]
    pub host: String,
    /// Port to listen on. Default: 3000
    #[serde(default = "default_port")]
    pub port: u16,
    /// PostgreSQL connection string
    pub database_url: String,
    /// Maximum database pool connections. Default: 5
    #[serde(default = "default_max_db_connections")]
    pub max_db_connections: u32,
    /// Allowed CORS origins. Default: ["http://localhost:1420"]
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
    /// Tracing log level. Default: "info"
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    3000
}
fn default_max_db_connections() -> u32 {
    5
}
fn default_cors_origins() -> Vec<String> {
    vec!["http://localhost:1420".to_string()]
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database_url: String::new(),
            max_db_connections: default_max_db_connections(),
            cors_origins: default_cors_origins(),
            log_level: default_log_level(),
        }
    }
}

impl ServerConfig {
    /// Load configuration from TOML file with environment variable overrides.
    ///
    /// Reads `config.toml` from CWD (or path in `CONFIG_PATH` env var),
    /// then overrides individual fields from env vars.
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
        let contents = std::fs::read_to_string(&path)?;
        Self::from_toml_str(&contents)
    }

    /// Load configuration from a TOML string, then apply env var overrides.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut config: ServerConfig = toml::from_str(toml_str)?;
        config.apply_env_overrides()?;
        Ok(config)
    }

    /// Apply environment variable overrides to the config.
    ///
    /// Returns an error if an env var is set but has an invalid format
    /// (e.g., PORT=abc).
    pub fn apply_env_overrides(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(val) = std::env::var("HOST") {
            self.host = val;
        }
        if let Ok(val) = std::env::var("PORT") {
            self.port = val
                .parse()
                .map_err(|_| format!("invalid PORT value: {val}"))?;
        }
        if let Ok(val) = std::env::var("DATABASE_URL") {
            self.database_url = val;
        }
        if let Ok(val) = std::env::var("MAX_DB_CONNECTIONS") {
            self.max_db_connections = val
                .parse()
                .map_err(|_| format!("invalid MAX_DB_CONNECTIONS value: {val}"))?;
        }
        if let Ok(val) = std::env::var("LOG_LEVEL") {
            self.log_level = val;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_loads_from_valid_toml_string() {
        let toml = r#"
            host = "0.0.0.0"
            port = 8080
            database_url = "postgresql://user:pass@localhost/db"
            max_db_connections = 10
            cors_origins = ["http://localhost:3000"]
            log_level = "debug"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.database_url, "postgresql://user:pass@localhost/db");
        assert_eq!(config.max_db_connections, 10);
        assert_eq!(config.cors_origins, vec!["http://localhost:3000"]);
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn test_config_applies_env_var_overrides() {
        let toml = r#"
            database_url = "postgresql://original@localhost/db"
        "#;
        std::env::set_var("DATABASE_URL", "postgresql://overridden@localhost/db");
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.database_url, "postgresql://overridden@localhost/db");
        std::env::remove_var("DATABASE_URL");
    }

    #[test]
    fn test_config_has_correct_defaults_for_omitted_fields() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
        assert_eq!(config.max_db_connections, 5);
        assert_eq!(config.cors_origins, vec!["http://localhost:1420"]);
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn test_config_fails_on_malformed_toml() {
        let toml = "this is not valid = [[[toml";
        let result = ServerConfig::from_toml_str(toml);
        assert!(result.is_err());
    }
}
