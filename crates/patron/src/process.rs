use std::{
    io::{self, Read, Seek},
    path::Path,
    process::Stdio,
    time::Duration,
};

use common::hash;
use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use os_info::Type;
use reqwest::{
    multipart::{Form, Part},
    Client,
};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::{
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader},
    process::Command,
};

use crate::{
    archiver::{build_zip_archive, ArchiverError},
    config::{AuthenticationConfig, ProjectConfig},
};

/// `cargo-contract` repository used to install the potentially missing `cargo-contract` binary.
const CARGO_CONTRACT_REPO: &str = "https://github.com/paritytech/cargo-contract";

/// Default value passed to weight configuration flags of the `cargo-contract`.
const DEFAULT_WEIGHT_VAL: u64 = 10_000_000_000;

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

    /// Relative project directory used to build multi-contract projects.
    project_directory: Option<&'a str>,
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
/// This method returns [`FinishedBuildSession`], which contains WASM blob, JSON metadata and the resulting code hash.
pub(crate) async fn remote_build(
    auth_config: &AuthenticationConfig,
    project_config: &ProjectConfig,
    progress: &ProgressBar,
    force_new_build_sessions: bool,
    project_directory: Option<&Path>,
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

    let response = Client::new()
        .get(format!("{server_path}/buildSessions/latest/{archive_hash}"))
        .bearer_auth(auth_config.token())
        .send()
        .await?;

    let code_hash = if response.status().is_success() && !force_new_build_sessions {
        let json: ExistingCodeHashResponse = response.json().await?;
        json.code_hash
    } else {
        let (file, _path) = archive_file.into_parts();

        let mut tokio_file = tokio::fs::File::from_std(file);
        tokio_file.seek(std::io::SeekFrom::Start(0)).await?;
        let length = tokio_file.metadata().await?.len();

        let source_code_body = Form::new().part(
            "archive",
            Part::stream_with_length(tokio_file, length).mime_str("application/zip")?,
        );

        progress.set_message("Uploading source code...");

        let source_code_upload: CreateResponse = Client::new()
            .post(format!("{server_path}/sourceCode"))
            .bearer_auth(auth_config.token())
            .multipart(source_code_body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        progress.set_message("Creating build session...");

        let build_session_create: CreateResponse = Client::new()
            .post(format!("{server_path}/buildSessions"))
            .bearer_auth(auth_config.token())
            .json(&BuildSessionCreateRequest {
                source_code_id: source_code_upload.id,
                cargo_contract_version: &project_config.cargo_contract_version,
                project_directory: project_directory
                    .map(|p| p.display().to_string())
                    .as_deref(),
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut log_position = 0;

        progress.set_message("Awaiting for build to finish...");

        loop {
            let logs: BuildSessionLogs = Client::new()
                .get(format!(
                    "{server_path}/buildSessions/logs/{}",
                    build_session_create.id
                ))
                .query(&[("position", log_position)])
                .bearer_auth(auth_config.token())
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

            for log in &logs.logs {
                progress.suspend(|| print!("{}", log.text));
            }

            if let Some(log) = logs.logs.last() {
                log_position = log.id;
            }

            let build_session_status: BuildSessionStatus = Client::new()
                .get(format!(
                    "{server_path}/buildSessions/status/{}",
                    build_session_create.id
                ))
                .bearer_auth(auth_config.token())
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;

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

    let wasm_file = tempfile::Builder::new().suffix(".wasm").tempfile()?;
    let metadata_file = tempfile::Builder::new().suffix(".json").tempfile()?;

    let wasm = Client::new()
        .get(format!("{server_path}/buildSessions/wasm/{}", code_hash))
        .bearer_auth(auth_config.token())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let wasm_file = write_to_tempfile(wasm_file, &wasm).await?;

    let metadata = Client::new()
        .get(format!(
            "{server_path}/buildSessions/metadata/{}",
            code_hash
        ))
        .bearer_auth(auth_config.token())
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let metadata_file = write_to_tempfile(metadata_file, &metadata).await?;

    Ok(FinishedBuildSession {
        wasm_file,
        metadata_file,
        code_hash,
    })
}

/// Write the provided buffer to [`NamedTempFile`] in asynchronous manner.
///
/// This function internally converts [`NamedTempFile`] to a regular [`std::fs::File`],
/// which itself is then converted to [`tokio::fs::File`] for writing purposes.
///
/// To ensure that [`NamedTempFile`] gets deleted in a RAII manner, convertion operations
/// are done in reverse as soon as the writing process gets finished, and the resulting
/// [`NamedTempFile`] is returned from this function.
async fn write_to_tempfile(
    file: NamedTempFile,
    mut val: &[u8],
) -> Result<NamedTempFile, io::Error> {
    let (file, path) = file.into_parts();

    let mut tokio_file = tokio::fs::File::from_std(file);

    let result = tokio::io::copy(&mut val, &mut tokio_file).await;

    let temp_file = NamedTempFile::from_parts(tokio_file.into_std().await, path);

    result.map(|_| temp_file)
}

/// Errors related to the contract build process.
#[derive(Debug, Display, From, Error)]
pub(crate) enum BuildError {
    /// IO-related error.
    Io(io::Error),

    /// JSON result parsing error.
    Json(serde_json::Error),

    /// Contract could not be built.
    #[display(fmt = "unable to build a contract")]
    BuildError,
}

/// JSON output of a contract build process.
#[derive(Deserialize)]
pub(crate) struct BuildResult {
    /// Path to the WASM blob file.
    pub(crate) dest_wasm: String,

    /// Built contract metadata information.
    pub(crate) metadata_result: Metadata,
}

/// Nested metadata struct used in [`BuildResult`].
#[derive(Deserialize)]
pub(crate) struct Metadata {
    /// Path to the JSON metadata file.
    pub(crate) dest_metadata: String,
}

/// Build contract locally using `cargo-contract` and retrieve JSON value of metadata.
pub(crate) async fn build_locally(
    cargo: &Path,
    verifiable: bool,
) -> Result<BuildResult, BuildError> {
    let mut build_command = Command::new(cargo);

    build_command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args(["contract", "build", "--output-json"]);

    if verifiable {
        build_command.arg("--verifiable");
    }

    let spawned = build_command.spawn()?.wait_with_output().await?;

    if !spawned.status.success() {
        return Err(BuildError::BuildError);
    }

    Ok(serde_json::from_slice(&spawned.stdout)?)
}

/// Instantiation configuration.
pub(crate) struct Instantiation<'a> {
    /// Constructor to call.
    pub constructor: &'a str,

    /// Constructor arguments.
    pub args: Option<&'a str>,

    /// Substrate node URI.
    pub suri: Option<&'a str>,

    /// Substrate node URL.
    pub url: Option<&'a str>,

    /// Gas value used to instantiate the contract.
    pub gas: Option<u64>,

    /// Maximum proof size for contract instantiation.
    pub proof_size: Option<u64>,
}

/// Errors related to the contract instantiation process.
#[derive(Debug, Display, From, Error)]
pub(crate) enum InstantiationError {
    /// IO-related error.
    Io(io::Error),

    /// JSON result parsing error.
    Json(serde_json::Error),

    /// Contract could not be instantiated from the downloaded WASM blob.
    #[display(fmt = "unable to instantiate a contract")]
    InstantiationError,
}

/// JSON output of a contract instantiation process.
#[derive(Deserialize)]
struct InstantiationResult {
    /// Contract address.
    contract: String,
}

/// Instantiate a contract
pub(crate) async fn instantiate_contract(
    cargo: &Path,
    instantiation: &Instantiation<'_>,
    cargo_contract_flags: &[String],
    metadata_path: Option<&Path>,
    salt: u64,
) -> Result<String, InstantiationError> {
    let mut instantiate_command = Command::new(cargo);

    instantiate_command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .args([
            "contract",
            "instantiate",
            "--execute",
            "--output-json",
            "--skip-confirm",
            "--skip-dry-run",
            "--gas",
            &instantiation.gas.unwrap_or(DEFAULT_WEIGHT_VAL).to_string(),
            "--proof-size",
            &instantiation
                .proof_size
                .unwrap_or(DEFAULT_WEIGHT_VAL)
                .to_string(),
            "--salt",
            &hex::encode(salt.to_le_bytes()),
        ])
        .args(["--constructor", &instantiation.constructor])
        .args(cargo_contract_flags);

    if let Some(metadata_path) = metadata_path {
        instantiate_command.arg(metadata_path);
    }

    if let Some(url) = instantiation.url {
        instantiate_command.args(["--url", url]);
    }

    if let Some(suri) = instantiation.suri {
        instantiate_command.args(["--suri", suri]);
    }

    if let Some(args) = instantiation.args {
        instantiate_command.args(["--args", &args]);
    }

    let spawned = instantiate_command.spawn()?.wait_with_output().await?;

    if !spawned.status.success() {
        return Err(InstantiationError::InstantiationError);
    }

    let parsed_output: InstantiationResult = serde_json::from_slice(&spawned.stdout)?;

    Ok(parsed_output.contract)
}

/// Errors that may occur during the `cargo-contract` installation phase.
#[derive(Debug, Display, From, Error)]
pub(crate) enum CargoContractInstallError {
    /// IO-related error.
    Io(io::Error),

    /// Unable to install `cargo-contract`.
    InstallationError,

    /// `cargo-contract` output was unexpected and cannot be used to determine version.
    #[display(fmt = "invalid cargo-contract output")]
    InvalidCargoContractOutput,
}

/// Ensure `cargo-contract` exists, installing it automatically if it isn't.
pub(crate) async fn ensure_cargo_contract_exists(
    cargo: &Path,
    cargo_contract_version: &str,
    progress: &ProgressBar,
) -> Result<(), CargoContractInstallError> {
    progress.set_message("Installing cargo-contract...");

    let cargo_contract_output = Command::new(cargo)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .args(["contract", "--version"])
        .spawn()?
        .wait_with_output()
        .await?;

    let should_reinstall = !cargo_contract_output.status.success() || {
        let output = String::from_utf8(cargo_contract_output.stdout)
            .map_err(|_| CargoContractInstallError::InvalidCargoContractOutput)?;

        !output
            .split_ascii_whitespace()
            .nth(1)
            .ok_or(CargoContractInstallError::InvalidCargoContractOutput)?
            .starts_with(cargo_contract_version)
    };

    if should_reinstall {
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
        let mut lines = logs.lines();

        while let Some(line) = lines.next_line().await? {
            progress.println(line);
        }

        if !install_command.wait().await?.success() {
            return Err(CargoContractInstallError::InstallationError);
        }
    }

    Ok(())
}

/// Ensure Docker exists, assisting user with its installation if it was not found.
pub(crate) async fn ensure_docker_exists() -> bool {
    let docker_exists = Command::new("docker")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("--version")
        .spawn()
        .map(|val| val.wait_with_output());

    let should_provide_guide = if let Ok(val) = docker_exists {
        val.await.is_err()
    } else {
        true
    };

    if should_provide_guide {
        let os_type = os_info::get().os_type();

        let instructions = match os_type {
            Type::Ubuntu => "https://docs.docker.com/desktop/install/ubuntu/",
            Type::Debian => "https://docs.docker.com/desktop/install/debian/",
            Type::Fedora => "https://docs.docker.com/desktop/install/fedora/",
            Type::Arch => "https://docs.docker.com/desktop/install/archlinux/",
            Type::Windows => "https://docs.docker.com/desktop/install/windows-install/",
            Type::Macos => "https://docs.docker.com/desktop/install/mac-install/",
            _ => "https://docs.docker.com/desktop/install/linux-install/",
        };

        println!("It seems that you don't have a Docker installation available.");
        println!("Detected OS: {os_type}");
        println!("Consult {instructions} for more information on how to install Docker on your local machine.");

        true
    } else {
        false
    }
}
