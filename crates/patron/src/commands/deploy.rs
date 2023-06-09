use std::{
    io::{self, BufRead, BufReader, Read, Seek},
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
    commands::Deploy,
    config::{AuthenticationConfig, AuthenticationConfigError, ProjectConfig},
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

    /// Preferred Rust toolchain version.
    rustc_version: &'a str,

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
pub(crate) enum DeployError {
    /// Authentication configuration error.
    Authentication(AuthenticationConfigError),

    /// Unable to parse the project configuration with [`figment`].
    Figment(figment::Error),

    /// IO-related error.
    Io(io::Error),

    /// HTTP client error.
    Http(reqwest::Error),

    /// Zip archiver error.
    #[display(fmt = "unable to create zip archive: {}", _0)]
    Archiver(ArchiverError),

    /// [`which`] crate was unable to determine location of the `cargo` binary file.
    #[display(fmt = "unable to locate cargo: {}", _0)]
    Which(which::Error),

    /// Unable to install `cargo-contract` binary.
    #[display(fmt = "unable to install cargo-contract")]
    CargoContractInstallError,

    /// Contract could not be instantiated from the downloaded WASM blob.
    #[display(fmt = "unable to instantiate a contract")]
    InstantiationError,
}

/// Deployment flow entrypoint.
pub(crate) fn deploy(
    Deploy {
        constructor,
        force_new_build_sessions,
        url,
        suri,
        args,
        gas,
        proof_size,
        cargo_contract_flags,
    }: Deploy,
) -> Result<(), DeployError> {
    let auth_config = AuthenticationConfig::new()?;
    let project_config = ProjectConfig::new()?;

    let server_path = auth_config.server_path();

    let cargo = which::which("cargo")?;

    let pg = ProgressBar::new_spinner();

    pg.enable_steady_tick(Duration::from_millis(150));
    pg.set_message("Archiving...");

    let mut archive_file = NamedTempFile::new()?;

    build_zip_archive(&mut archive_file, &pg)?;

    let mut archive_buf = Vec::with_capacity(archive_file.stream_position()? as usize);
    archive_file.seek(std::io::SeekFrom::Start(0))?;
    archive_file.read_to_end(&mut archive_buf)?;
    let archive_hash = hex::encode(hash::blake2(&archive_buf));

    pg.set_message("Retrieving existing build session...");

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

        pg.set_message("Uploading source code...");

        let source_code_upload: CreateResponse = reqwest::blocking::Client::new()
            .post(format!("{server_path}/sourceCode"))
            .bearer_auth(auth_config.token())
            .multipart(source_code_body)
            .send()?
            .error_for_status()?
            .json()?;

        pg.set_message("Creating build session...");

        let build_session_create: CreateResponse = reqwest::blocking::Client::new()
            .post(format!("{server_path}/buildSessions"))
            .bearer_auth(auth_config.token())
            .json(&BuildSessionCreateRequest {
                source_code_id: source_code_upload.id,
                rustc_version: &project_config.rustc_version,
                cargo_contract_version: &project_config.cargo_contract_version,
            })
            .send()?
            .error_for_status()?
            .json()?;

        let mut log_position = 0;

        pg.set_message("Awaiting for build to finish...");

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
                pg.suspend(|| print!("{}", log.text));
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
                    pg.finish_with_message("Build failed.");
                    return Ok(());
                }
                _ => {}
            }

            std::thread::sleep(Duration::from_secs(3));
        }
    };

    pg.set_message("Installing cargo-contract...");

    let cargo_contract_version = Command::new(&cargo)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(["contract", "--version"])
        .spawn()?
        .wait()?;

    if !cargo_contract_version.success() {
        let mut install_command = Command::new(&cargo)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args([
                "install",
                "cargo-contract",
                "--git",
                CARGO_CONTRACT_REPO,
                "--tag",
                &format!("v{}", &project_config.cargo_contract_version),
            ])
            .spawn()?;

        let logs = BufReader::new(install_command.stderr.take().unwrap());

        for log in logs.lines() {
            pg.println(log?);
        }

        if !install_command.wait()?.success() {
            return Err(DeployError::CargoContractInstallError);
        }
    }

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

    pg.set_message("Deploying...");

    let mut upload_command = Command::new(&cargo);

    upload_command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args([
            "contract",
            "upload",
            "--execute",
            "--skip-confirm",
            "--skip-dry-run",
        ])
        .arg(wasm_file.path())
        .args(&cargo_contract_flags);

    if let Some(url) = url.as_deref() {
        upload_command.args(["--url", url]);
    }

    if let Some(suri) = suri.as_deref() {
        upload_command.args(["--suri", suri]);
    }

    upload_command.spawn()?.wait()?;

    // Don't check for upload errors, since we might already have
    // the same code hash uploaded. Proceed with instantiation instead.
    let mut instantiate_command = Command::new(cargo);

    instantiate_command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args([
            "contract",
            "instantiate",
            "--execute",
            "--skip-confirm",
            "--skip-dry-run",
            "--gas",
            &gas.unwrap_or(DEFAULT_WEIGHT_VAL).to_string(),
            "--proof-size",
            &proof_size.unwrap_or(DEFAULT_WEIGHT_VAL).to_string(),
        ])
        .arg(metadata_file.path())
        .args(["--constructor", &constructor])
        .args(cargo_contract_flags);

    if let Some(url) = url.as_deref() {
        instantiate_command.args(["--url", url]);
    }

    if let Some(suri) = suri.as_deref() {
        instantiate_command.args(["--suri", suri]);
    }

    if let Some(args) = args {
        instantiate_command.args(["--args", &args]);
    }

    if !instantiate_command.spawn()?.wait()?.success() {
        return Err(DeployError::InstantiationError);
    }

    pg.finish_with_message(format!(
        "Contract uploaded: {}/codeHash/{}",
        auth_config.web_path(),
        code_hash
    ));

    Ok(())
}
