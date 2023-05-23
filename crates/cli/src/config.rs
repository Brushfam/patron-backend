use std::{fs, io, path::PathBuf};

use derive_more::{Display, Error, From};
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Display, From, Error)]
pub enum AuthenticationConfigError {
    Figment(figment::Error),
    Io(io::Error),
    Toml(toml::ser::Error),

    #[display(fmt = "unable to find home directory")]
    HomeDirNotFound,
}

#[derive(Serialize, Deserialize)]
pub struct AuthenticationConfig {
    /// Authentication token.
    token: String,

    /// Custom server path specification.
    server_path: String,

    /// Custom web path specification.
    web_path: String,
}

pub fn default_server_path() -> String {
    String::from("https://api.patron.works")
}

pub fn default_web_path() -> String {
    String::from("https://patron.works")
}

impl AuthenticationConfig {
    pub fn new() -> Result<Self, AuthenticationConfigError> {
        Ok(Figment::new()
            .merge(Toml::file(Self::config_path()?))
            .merge(Env::prefixed("AUTH_"))
            .extract()?)
    }

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

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn server_path(&self) -> &str {
        &self.server_path
    }

    fn config_path() -> Result<PathBuf, AuthenticationConfigError> {
        let mut home_dir = home::home_dir().ok_or(AuthenticationConfigError::HomeDirNotFound)?;
        home_dir.push(".ink-deploy/auth.toml");
        Ok(home_dir)
    }
}

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
