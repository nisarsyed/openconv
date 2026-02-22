use serde::Deserialize;

// ---------------------------------------------------------------------------
// Sub-struct: Redis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-struct: JWT
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    /// Ed25519 private key PEM -- MUST come from JWT_PRIVATE_KEY_PEM env var
    #[serde(default)]
    pub private_key_pem: String,
    /// Ed25519 public key PEM -- MUST come from JWT_PUBLIC_KEY_PEM env var
    #[serde(default)]
    pub public_key_pem: String,
    /// Access token lifetime in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_access_ttl")]
    pub access_token_ttl_seconds: u64,
    /// Refresh token lifetime in seconds (default: 604800 = 7 days)
    #[serde(default = "default_refresh_ttl")]
    pub refresh_token_ttl_seconds: u64,
}

fn default_access_ttl() -> u64 {
    300
}
fn default_refresh_ttl() -> u64 {
    604800
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            private_key_pem: String::new(),
            public_key_pem: String::new(),
            access_token_ttl_seconds: default_access_ttl(),
            refresh_token_ttl_seconds: default_refresh_ttl(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-struct: Email
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_username: String,
    /// MUST come from SMTP_PASSWORD env var
    #[serde(default)]
    pub smtp_password: String,
    #[serde(default)]
    pub from_address: String,
    #[serde(default = "default_from_name")]
    pub from_name: String,
}

fn default_smtp_port() -> u16 {
    587
}
fn default_from_name() -> String {
    "OpenConv".to_string()
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            smtp_username: String::new(),
            smtp_password: String::new(),
            from_address: String::new(),
            from_name: default_from_name(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-struct: Rate Limiting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_ip_limit")]
    pub auth_per_ip_per_minute: u32,
    #[serde(default = "default_key_limit")]
    pub challenge_per_key_per_minute: u32,
    #[serde(default = "default_email_limit")]
    pub email_per_address_per_hour: u32,
    #[serde(default = "default_guild_limit")]
    pub guild_per_user_per_minute: u32,
    #[serde(default = "default_channel_limit")]
    pub channel_per_user_per_minute: u32,
    #[serde(default = "default_file_limit")]
    pub file_per_user_per_minute: u32,
    #[serde(default = "default_invite_limit")]
    pub invite_per_user_per_hour: u32,
}

fn default_ip_limit() -> u32 {
    30
}
fn default_key_limit() -> u32 {
    5
}
fn default_email_limit() -> u32 {
    3
}
fn default_guild_limit() -> u32 {
    10
}
fn default_channel_limit() -> u32 {
    20
}
fn default_file_limit() -> u32 {
    10
}
fn default_invite_limit() -> u32 {
    10
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            auth_per_ip_per_minute: default_ip_limit(),
            challenge_per_key_per_minute: default_key_limit(),
            email_per_address_per_hour: default_email_limit(),
            guild_per_user_per_minute: default_guild_limit(),
            channel_per_user_per_minute: default_channel_limit(),
            file_per_user_per_minute: default_file_limit(),
            invite_per_user_per_hour: default_invite_limit(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-struct: File Storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct FileStorageConfig {
    #[serde(default = "default_storage_backend")]
    pub backend: String,
    #[serde(default = "default_local_path")]
    pub local_path: String,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_bytes: u64,
}

fn default_storage_backend() -> String {
    "local".to_string()
}
fn default_local_path() -> String {
    "./data/files".to_string()
}
fn default_max_file_size() -> u64 {
    26_214_400 // 25MB
}

impl Default for FileStorageConfig {
    fn default() -> Self {
        Self {
            backend: default_storage_backend(),
            local_path: default_local_path(),
            max_file_size_bytes: default_max_file_size(),
        }
    }
}

// ---------------------------------------------------------------------------
// Main ServerConfig
// ---------------------------------------------------------------------------

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

    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub jwt: JwtConfig,
    #[serde(default)]
    pub email: EmailConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub file_storage: FileStorageConfig,
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
            redis: RedisConfig::default(),
            jwt: JwtConfig::default(),
            email: EmailConfig::default(),
            rate_limit: RateLimitConfig::default(),
            file_storage: FileStorageConfig::default(),
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
        if let Ok(val) = std::env::var("JWT_PRIVATE_KEY_PEM") {
            self.jwt.private_key_pem = val;
        }
        if let Ok(val) = std::env::var("JWT_PUBLIC_KEY_PEM") {
            self.jwt.public_key_pem = val;
        }
        if let Ok(val) = std::env::var("SMTP_PASSWORD") {
            self.email.smtp_password = val;
        }
        if let Ok(val) = std::env::var("REDIS_URL") {
            self.redis.url = val;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
    #[serial]
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

    #[test]
    fn test_config_parses_nested_redis_section() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
            [redis]
            url = "redis://localhost:6380"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.redis.url, "redis://localhost:6380");
    }

    #[test]
    fn test_config_parses_nested_jwt_section() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
            [jwt]
            access_token_ttl_seconds = 600
            refresh_token_ttl_seconds = 86400
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.jwt.access_token_ttl_seconds, 600);
        assert_eq!(config.jwt.refresh_token_ttl_seconds, 86400);
    }

    #[test]
    fn test_config_parses_nested_email_section() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
            [email]
            smtp_host = "smtp.example.com"
            smtp_port = 587
            smtp_username = "user"
            from_address = "noreply@example.com"
            from_name = "OpenConv"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.email.smtp_host, "smtp.example.com");
        assert_eq!(config.email.smtp_port, 587);
    }

    #[test]
    fn test_config_parses_nested_rate_limit_section() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
            [rate_limit]
            auth_per_ip_per_minute = 60
            challenge_per_key_per_minute = 10
            email_per_address_per_hour = 5
            guild_per_user_per_minute = 15
            channel_per_user_per_minute = 30
            file_per_user_per_minute = 5
            invite_per_user_per_hour = 20
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.rate_limit.auth_per_ip_per_minute, 60);
        assert_eq!(config.rate_limit.guild_per_user_per_minute, 15);
        assert_eq!(config.rate_limit.channel_per_user_per_minute, 30);
        assert_eq!(config.rate_limit.file_per_user_per_minute, 5);
        assert_eq!(config.rate_limit.invite_per_user_per_hour, 20);
    }

    #[test]
    fn test_rate_limit_user_fields_have_correct_defaults() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.rate_limit.guild_per_user_per_minute, 10);
        assert_eq!(config.rate_limit.channel_per_user_per_minute, 20);
        assert_eq!(config.rate_limit.file_per_user_per_minute, 10);
        assert_eq!(config.rate_limit.invite_per_user_per_hour, 10);
    }

    #[test]
    fn test_config_still_parses_minimal_config_with_defaults() {
        let toml = r#"
            database_url = "postgresql://localhost/db"
        "#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.redis.url, "redis://localhost:6379");
        assert_eq!(config.jwt.access_token_ttl_seconds, 300);
        assert_eq!(config.jwt.refresh_token_ttl_seconds, 604800);
        assert_eq!(config.rate_limit.auth_per_ip_per_minute, 30);
    }

    #[test]
    #[serial]
    fn test_jwt_pem_keys_from_env_vars() {
        std::env::set_var("JWT_PRIVATE_KEY_PEM", "test-private-pem");
        std::env::set_var("JWT_PUBLIC_KEY_PEM", "test-public-pem");
        let toml = r#"database_url = "postgresql://localhost/db""#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.jwt.private_key_pem, "test-private-pem");
        assert_eq!(config.jwt.public_key_pem, "test-public-pem");
        std::env::remove_var("JWT_PRIVATE_KEY_PEM");
        std::env::remove_var("JWT_PUBLIC_KEY_PEM");
    }

    #[test]
    #[serial]
    fn test_smtp_password_from_env_var() {
        std::env::set_var("SMTP_PASSWORD", "secret-smtp-pass");
        let toml = r#"database_url = "postgresql://localhost/db""#;
        let config = ServerConfig::from_toml_str(toml).unwrap();
        assert_eq!(config.email.smtp_password, "secret-smtp-pass");
        std::env::remove_var("SMTP_PASSWORD");
    }

    #[test]
    fn test_default_access_token_ttl_is_300() {
        let jwt = JwtConfig::default();
        assert_eq!(jwt.access_token_ttl_seconds, 300);
    }

    #[test]
    fn test_default_refresh_token_ttl_is_604800() {
        let jwt = JwtConfig::default();
        assert_eq!(jwt.refresh_token_ttl_seconds, 604800);
    }
}
