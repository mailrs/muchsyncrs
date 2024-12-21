#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error")]
    Config(#[from] crate::config::ConfigError),
}
