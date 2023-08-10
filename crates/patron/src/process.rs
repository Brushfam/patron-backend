use std::{
    io::{self, BufRead, BufReader, Read, Seek},
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use common::hash;
use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use reqwest::blocking::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    archiver::{build_zip_archive, ArchiverError},
    config::{AuthenticationConfig, ProjectConfig},
};

/// `cargo-contract` repository used to install the potentially missing `cargo-contract` binary.
const CARGO_CONTRACT_REPO: &str = "https://github.com/paritytech/cargo-contract";

/// JSON response body with the code hash of a cached build session that matches some source code.
#[derive(Deserialize)]
struct ExistingCodeHashResponse {
    /// Code hash hex-encoded value.
    code_hash: String,
}

/// JSON response body returned by build session creation and source code upload requests.
#[derive(Deserialize)]
struct CreateResponse {
    /// Resource identifier.
    id: i64,
}

/// JSON request body that is used to create a new build session.
#[derive(Serialize)]
struct BuildSessionCreateRequest<'a> {
    /// Source code identifier to build from.
    source_code_id: i64,

    /// Preferred `cargo-contract` version.
    cargo_contract_version: &'a str,
}

/// JSON response body with the status of an initiated build session.
#[derive(Deserialize)]
struct BuildSessionStatus {
    /// Current build session status.
    ///
    /// For an enumeration of supported values see the `db` crate documentation.
    status: String,

    /// Build session code hash, if the build was completed successfully.
    code_hash: Option<String>,
}

/// JSON response body with build session logs.
#[derive(Deserialize)]
struct BuildSessionLogs {
    /// Contained build session logs.
    logs: Vec<BuildSessionLog>,
}

/// A single build session log entry.
#[derive(Deserialize)]
struct BuildSessionLog {
    /// Log entry identifier, that can be used to paginate over build session logs.
    id: i64,

    /// Log entry text value.
    text: String,
}

/// `deploy` subcommand errors.
#[derive(Debug, Display, From, Error)]
pub(crate) enum RemoteBuildError {
    /// IO-related error.
    Io(io::Error),

    /// HTTP client error.
    Http(reqwest::Error),

    /// Zip archiver error.
    #[display(fmt = "unable to create zip archive: {}", _0)]
    Archiver(ArchiverError),

    /// Build session failed.
    #[display(fmt = "unable to finish this build session")]
    BuildFailed,
}

/// Finished remote build session.
pub(crate) struct FinishedBuildSession {
    /// Downloaded WASM blob from a remote build session.
    pub wasm_file: NamedTempFile,

    /// Downloaded JSON metadata from a remote build session.
    pub metadata_file: NamedTempFile,

    /// Code hash value of a resulted WASM blob.
    pub code_hash: String,
}

/// Start remote build process.
///
/// This method returns two [`NamedTempFile`]'s which correspond to a WASM
/// blob and a JSON metadata.
pub(crate) fn remote_build(
    auth_config: &AuthenticationConfig,
    project_config: &ProjectConfig,
    progress: &ProgressBar,
    force_new_build_sessions: bool,
) -> Result<FinishedBuildSession, RemoteBuildError> {
    let server_path = auth_config.server_path();

    progress.enable_steady_tick(Duration::from_millis(150));
    progress.set_message("Archiving...");

    let mut archive_file = NamedTempFile::new()?;

    build_zip_archive(&mut archive_file, progress)?;

    let mut archive_buf = Vec::with_capacity(archive_file.stream_position()? as usize);
    archive_file.seek(std::io::SeekFrom::Start(0))?;
    archive_file.read_to_end(&mut archive_buf)?;
    let archive_hash = hex::encode(hash::blake2(&archive_buf));

    progress.set_message("Retrieving existing build session...");

    let response = reqwest::blocking::Client::new()
        .get(format!("{server_path}/buildSessions/latest/{archive_hash}"))
        .bearer_auth(auth_config.token())
        .send()?;

    let code_hash = if response.status().is_success() && !force_new_build_sessions {
        let json: ExistingCodeHashResponse = response.json()?;
        json.code_hash
    } else {
        let source_code_body = Form::new().part(
            "archive",
            Part::file(archive_file.path())?.mime_str("application/zip")?,
        );

        progress.set_message("Uploading source code...");

        let source_code_upload: CreateResponse = reqwest::blocking::Client::new()
            .post(format!("{server_path}/sourceCode"))
            .bearer_auth(auth_config.token())
            .multipart(source_code_body)
            .send()?
            .error_for_status()?
            .json()?;

        progress.set_message("Creating build session...");

        let build_session_create: CreateResponse = reqwest::blocking::Client::new()
            .post(format!("{server_path}/buildSessions"))
            .bearer_auth(auth_config.token())
            .json(&BuildSessionCreateRequest {
                source_code_id: source_code_upload.id,
                cargo_contract_version: &project_config.cargo_contract_version,
            })
            .send()?
            .error_for_status()?
            .json()?;

        let mut log_position = 0;

        progress.set_message("Awaiting for build to finish...");

        loop {
            let logs: BuildSessionLogs = reqwest::blocking::Client::new()
                .get(format!(
                    "{server_path}/buildSessions/logs/{}",
                    build_session_create.id
                ))
                .query(&[("position", log_position)])
                .bearer_auth(auth_config.token())
                .send()?
                .error_for_status()?
                .json()?;

            for log in &logs.logs {
                progress.suspend(|| print!("{}", log.text));
            }

            if let Some(log) = logs.logs.last() {
                log_position = log.id;
            }

            let build_session_status: BuildSessionStatus = reqwest::blocking::Client::new()
                .get(format!(
                    "{server_path}/buildSessions/status/{}",
                    build_session_create.id
                ))
                .bearer_auth(auth_config.token())
                .send()?
                .error_for_status()?
                .json()?;

            match (
                &*build_session_status.status,
                build_session_status.code_hash,
            ) {
                ("completed", Some(code_hash)) => break code_hash,
                ("failed", _) => {
                    progress.finish_with_message("Build failed.");
                    return Err(RemoteBuildError::BuildFailed);
                }
                _ => {}
            }

            std::thread::sleep(Duration::from_secs(3));
        }
    };

    let mut wasm_file = tempfile::Builder::new().suffix(".wasm").tempfile()?;
    let mut metadata_file = tempfile::Builder::new().suffix(".json").tempfile()?;

    reqwest::blocking::Client::new()
        .get(format!("{server_path}/buildSessions/wasm/{}", code_hash))
        .bearer_auth(auth_config.token())
        .send()?
        .error_for_status()?
        .copy_to(wasm_file.as_file_mut())?;

    reqwest::blocking::Client::new()
        .get(format!(
            "{server_path}/buildSessions/metadata/{}",
            code_hash
        ))
        .bearer_auth(auth_config.token())
        .send()?
        .error_for_status()?
        .copy_to(metadata_file.as_file_mut())?;

    Ok(FinishedBuildSession {
        wasm_file,
        metadata_file,
        code_hash,
    })
}

/// Errors that may occur during the `cargo-contract` installation phase.
#[derive(Debug, Display, From, Error)]
pub(crate) enum CargoContractInstallError {
    /// IO-related error.
    Io(io::Error),

    /// Unable to install `cargo-contract`.
    InstallationError,
}

/// Ensure `cargo-contract` exists, installing it automatically if it isn't.
pub(crate) fn ensure_cargo_contract_exists(
    cargo: &Path,
    cargo_contract_version: &str,
    progress: &ProgressBar,
) -> Result<(), CargoContractInstallError> {
    progress.set_message("Installing cargo-contract...");

    let cargo_contract_exists = Command::new(cargo)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(["contract", "--version"])
        .spawn()?
        .wait()?;

    if !cargo_contract_exists.success() {
        let mut install_command = Command::new(cargo)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args([
                "install",
                "cargo-contract",
                "--git",
                CARGO_CONTRACT_REPO,
                "--tag",
                &format!("v{}", cargo_contract_version),
            ])
            .spawn()?;

        let logs = BufReader::new(install_command.stderr.take().unwrap());

        for log in logs.lines() {
            progress.println(log?);
        }

        if !install_command.wait()?.success() {
            return Err(CargoContractInstallError::InstallationError);
        }
    }

    Ok(())
}
