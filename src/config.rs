use dotenvy::dotenv;
use std::env;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Config {
    pub github_token: String,
    pub github_organization: String,
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
}
