use std::{fs, io, path::PathBuf};

use derive_more::{Display, Error, From};
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

/// Authentication configuration errors.
#[derive(Debug, Display, From, Error)]
pub enum AuthenticationConfigError {
    /// Unable to load the configuration using [`figment`].
    Figment(figment::Error),

    /// IO-related error.
    Io(io::Error),

    /// Unable to serialize the configuration using [`toml`] crate.
    Toml(toml::ser::Error),

    /// User's home directory cannot be determined.
    #[display(fmt = "unable to find home directory")]
    HomeDirNotFound,
}

/// Primary authentication config.
#[derive(Serialize, Deserialize)]
pub struct AuthenticationConfig {
    /// Authentication token.
    token: String,

    /// Custom server path specification.
    server_path: String,

    /// Custom web path specification.
    web_path: String,
}

/// Default server path for the hosted environment.
pub fn default_server_path() -> String {
    String::from("https://api.patron.works")
}

/// Default web UI path for the hosted environment.
pub fn default_web_path() -> String {
    String::from("https://patron.works")
}

impl AuthenticationConfig {
    /// Create new authentication config using default configuration file or environment variables.
    ///
    /// See [`Env`] for more details on how to use environment variables configuration.
    ///
    /// [`Env`]: figment::providers::Env
    pub fn new() -> Result<Self, AuthenticationConfigError> {
        Ok(Figment::new()
            .merge(Toml::file(Self::config_path()?))
            .merge(Env::prefixed("AUTH_"))
            .extract()?)
    }

    /// Write the configuration file to the default file location.
    pub fn write_token(
        token: String,
        server_path: String,
        web_path: String,
    ) -> Result<(), AuthenticationConfigError> {
        let path = Self::config_path()?;
        fs::create_dir_all(path.ancestors().nth(1).expect("incorrect config path"))?;
        fs::write(
            path,
            toml::to_string(&AuthenticationConfig {
                token,
                server_path,
                web_path,
            })?,
        )?;
        Ok(())
    }

    /// Get authentication token from the current configuration.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get API server path from the current configuration.
    pub fn server_path(&self) -> &str {
        &self.server_path
    }

    /// Get authentication configuration storage path.
    ///
    /// Returns [`Err`] if home directory cannot be determined.
    fn config_path() -> Result<PathBuf, AuthenticationConfigError> {
        let mut home_dir = home::home_dir().ok_or(AuthenticationConfigError::HomeDirNotFound)?;
        home_dir.push(".ink-deploy/auth.toml");
        Ok(home_dir)
    }
}

/// Project build configuration.
#[derive(Deserialize)]
pub struct ProjectConfig {
    /// `cargo-contract` package version.
    pub cargo_contract_version: String,

    /// Rust toolchain version.
    pub rustc_version: String,
}

impl ProjectConfig {
    /// Create new config using default configuration file.
    pub fn new() -> Result<Self, figment::Error> {
        Figment::new()
            .merge(Toml::file("Deploy.toml"))
            .merge(Env::prefixed("DEPLOY_"))
            .extract()
    }
}
