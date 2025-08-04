use dotenvy::dotenv;
use std::env;

#[derive(Debug, Clone)]
pub struct ApplicationConfig {
    pub github_token: String,
    pub github_organization: String,
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
}

#[derive(thiserror::Error, Debug)]
pub enum ApplicationConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),
    #[error("Invalid value for {variable}: {value}")]
    InvalidValue { variable: String, value: String },
}

impl ApplicationConfig {
    pub fn from_env() -> Result<Self, ApplicationConfigError> {
        dotenv().ok();
        Self::from_current_env()
    }

    pub fn from_current_env() -> Result<Self, ApplicationConfigError> {
        let github_token = env::var("GITHUB_TOKEN")
            .map_err(|_| ApplicationConfigError::MissingEnvVar("GITHUB_TOKEN".to_string()))?;

        let github_organization = env::var("GITHUB_ORGANIZATION")
            .map_err(|_| ApplicationConfigError::MissingEnvVar("GITHUB_ORGANIZATION".to_string()))?;

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| ApplicationConfigError::MissingEnvVar("DATABASE_URL".to_string()))?;

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|_| ApplicationConfigError::InvalidValue {
                variable: "SERVER_PORT".to_string(),
                value: env::var("SERVER_PORT").unwrap_or_default(),
            })?;

        Ok(ApplicationConfig {
            github_token,
            github_organization,
            database_url,
            server_host,
            server_port,
        })
    }

    pub fn validate(&self) -> Result<(), ApplicationConfigError> {
        if self.github_token.is_empty() {
            return Err(ApplicationConfigError::MissingEnvVar("GITHUB_TOKEN".to_string()));
        }

        if self.github_organization.is_empty() {
            return Err(ApplicationConfigError::MissingEnvVar(
                "GITHUB_ORGANIZATION".to_string(),
            ));
        }

        if self.database_url.is_empty() {
            return Err(ApplicationConfigError::MissingEnvVar("DATABASE_URL".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_config_from_env_missing_required() {
        let _guard = ENV_MUTEX.lock().unwrap();

        unsafe {
            env::remove_var("GITHUB_TOKEN");
            env::remove_var("GITHUB_ORGANIZATION");
            env::remove_var("DATABASE_URL");
            env::remove_var("SERVER_HOST");
            env::remove_var("SERVER_PORT");
        }

        let result = ApplicationConfig::from_current_env();
        dbg!(&result);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation() {
        let config = ApplicationConfig {
            github_token: "".to_string(),
            github_organization: "test-org".to_string(),
            database_url: "postgres://localhost/test".to_string(),
            server_host: "localhost".to_string(),
            server_port: 3000,
        };

        let result = config.validate();
        assert!(result.is_err());
    }
}
