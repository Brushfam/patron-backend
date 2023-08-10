use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
};

use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use serde_json::Value;
use tempfile::PersistError;

use crate::{
    commands::Build,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
    process::{remote_build, FinishedBuildSession, RemoteBuildError},
};

/// Directory, where build artifacts will be stored.
const TARGET_DIR: &str = "./target/ink";

/// Default path used to save WASM blob.
const DEFAULT_WASM_PATH: &str = "./target/ink/contract.wasm";

/// Default path used to save JSON metadata.
const DEFAULT_METADATA_PATH: &str = "./target/ink/contract.json";

/// Default path used to save bundled contract file.
const DEFAULT_BUNDLE_PATH: &str = "./target/ink/bundle.contract";

/// `build` subcommand errors.
#[derive(Debug, Display, From, Error)]
pub(crate) enum BuildError {
    /// Authentication configuration error.
    Authentication(AuthenticationConfigError),

    /// Unable to parse the project configuration with [`figment`].
    Figment(figment::Error),

    /// IO-related error.
    Io(io::Error),

    /// Metadata JSON parsing error.
    Json(serde_json::Error),

    /// Remote build process error.
    BuildProcessError(RemoteBuildError),

    /// Unable to move temporary files onto a new location.
    PersistError(PersistError),

    /// Invalid metadata object.
    #[display(fmt = "unable to retrieve the 'source' key from the metadata JSON")]
    InvalidMetadataObject,
}

/// Build flow entrypoint.
pub(crate) fn build(
    Build {
        force_new_build_sessions,
        wasm_path,
        metadata_path,
        bundle_path,
    }: Build,
) -> Result<(), BuildError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let progress = ProgressBar::new_spinner();

    let FinishedBuildSession {
        mut wasm_file,
        mut metadata_file,
        ..
    } = remote_build(
        &auth_config,
        &project_config,
        &progress,
        force_new_build_sessions,
    )?;

    if wasm_path.is_none() || metadata_path.is_none() || bundle_path.is_none() {
        fs::create_dir_all(TARGET_DIR)?;
    }

    wasm_file.seek(SeekFrom::Start(0))?;
    let mut wasm_buf = Vec::new();
    wasm_file.read_to_end(&mut wasm_buf)?;

    metadata_file.seek(SeekFrom::Start(0))?;
    let mut metadata: Value = serde_json::from_reader(&metadata_file)?;
    let wasm_hex = format!("0x{}", hex::encode(&wasm_buf));
    metadata["source"]
        .as_object_mut()
        .ok_or(BuildError::InvalidMetadataObject)?
        .insert("wasm".into(), Value::String(wasm_hex));

    // Ensure that cross-boundary filesystem copies are supported
    // by manually calling fs::copy.
    wasm_file.seek(SeekFrom::Start(0))?;

    fs::copy(
        &mut wasm_file,
        wasm_path.unwrap_or(PathBuf::from(DEFAULT_WASM_PATH)),
    )?;

    metadata_file.seek(SeekFrom::Start(0))?;

    fs::copy(
        &mut metadata_file,
        metadata_path.unwrap_or(PathBuf::from(DEFAULT_METADATA_PATH)),
    )?;

    serde_json::to_writer(
        File::create(bundle_path.unwrap_or(PathBuf::from(DEFAULT_BUNDLE_PATH)))?,
        &metadata,
    )?;

    Ok(())
}
