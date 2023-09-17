use std::{
    env::current_dir,
    fmt::Debug,
    fs::File,
    io::{self, BufReader},
    path::{Path, StripPrefixError},
};

use derive_more::{Display, Error, From};
use futures_util::SinkExt;
use indicatif::ProgressBar;
use itertools::Itertools;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use rand::{thread_rng, Rng};
use serde::Serialize;
use std::time::Duration;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, error::TryRecvError},
        watch::{
            self,
            error::{RecvError, SendError},
        },
    },
};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    commands::Watch,
    config::{default_web_path, ProjectConfig},
    process::{
        build_locally, ensure_cargo_contract_exists, instantiate_contract, BuildError,
        CargoContractInstallError, Instantiation, InstantiationError,
    },
};

/// `watch` subcommand errors.
#[derive(Debug, Display, From, Error)]
pub(crate) enum WatchError {
    /// IO-related error.
    Io(io::Error),

    /// [`which`] crate was unable to determine location of the `cargo` binary file.
    #[display(fmt = "unable to locate cargo: {}", _0)]
    Which(which::Error),

    /// Error while communicating with the [`notify`] crate.
    Notify(notify::Error),

    /// Unable to install `cargo-contract`.
    CargoContractInstallError(CargoContractInstallError),

    /// Unable to parse the project configuration with [`figment`].
    Figment(figment::Error),

    /// Channel is empty or disconnected.
    TryRecvError(TryRecvError),

    /// Channel is disconnected and message cannot be received.
    RecvError(RecvError),

    /// Channel is disconnected and message cannot be sended.
    SendError(SendError<Option<ContractInfo>>),

    /// JSON result parsing error.
    Json(serde_json::Error),

    /// Contract could not be built.
    #[display(fmt = "unable to build a contract: {}", _0)]
    BuildError(BuildError),

    /// Contract could not be instantiated.
    #[display(fmt = "unable to instantiate a contract: {}", _0)]
    InstantiationError(InstantiationError),

    /// Unable to strip path prefix since watcher was started in a different directory.
    #[display(fmt = "watcher started in a different directory: {}", _0)]
    WatcherStartedInADifferentDirectory(StripPrefixError),

    /// WebSocket error.
    #[display(fmt = "websocket error: {}", _0)]
    WebsocketError(tokio_tungstenite::tungstenite::Error),
}

/// Information about contract that gets transferred to WebSocket clients.
#[derive(Serialize)]
pub(crate) struct ContractInfo {
    /// Node RPC URL.
    node: String,

    /// Contract address.
    address: String,

    /// Contract metadata JSON value.
    metadata: serde_json::Value,
}

/// Watch for changes and deploy the contract.
pub(crate) async fn watch(config: Watch) -> Result<(), WatchError> {
    let web_domain = config.web_path.clone().unwrap_or_else(default_web_path);

    let _ = open::that_in_background(format!("{web_domain}/local-contract-caller"));

    let project_config = ProjectConfig::new()?;

    let (sender, receiver) = watch::channel(None);

    tokio::try_join!(
        websocket_server(receiver),
        watch_for_changes(&project_config, &config, sender)
    )?;

    Ok(())
}

/// Start WebSocket server.
///
/// This function spawns new task inside the Tokio runtime for each accepted connection.
async fn websocket_server(
    receiver: watch::Receiver<Option<ContractInfo>>,
) -> Result<(), WatchError> {
    let socket = TcpListener::bind("127.0.0.1:20600").await?;

    while let Ok((stream, _)) = socket.accept().await {
        tokio::spawn(handle_connection(stream, receiver.clone()));
    }

    Ok(())
}

/// Handle incoming WebSocket connection.
///
/// [`Future`] returned from this function is meant to be spawned inside
/// of the Tokio runtime.
///
/// [`Future`]: std::future::Future
async fn handle_connection(
    stream: TcpStream,
    mut receiver: watch::Receiver<Option<ContractInfo>>,
) -> Result<(), WatchError> {
    let mut ws_stream = tokio_tungstenite::accept_async(stream).await?;

    loop {
        let text = {
            receiver.changed().await?;

            let info = receiver.borrow_and_update();

            serde_json::to_string(info.as_ref().unwrap()).unwrap()
        };

        ws_stream.send(Message::Text(text)).await?;
    }
}

/// Start watching for file changes in the current directory.
async fn watch_for_changes(
    project_config: &ProjectConfig,
    Watch {
        constructor,
        args,
        suri,
        url,
        gas,
        proof_size,
        cargo_contract_flags,
        ..
    }: &Watch,
    info_sender: watch::Sender<Option<ContractInfo>>,
) -> Result<(), WatchError> {
    let progress = ProgressBar::new_spinner();

    let cargo = which::which("cargo")?;

    let pwd = current_dir()?;

    ensure_cargo_contract_exists(&cargo, &project_config.cargo_contract_version, &progress).await?;

    reset_progress(&progress);

    let (sender, mut receiver) = mpsc::channel(1);

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Some(event) = res.ok().filter(|event| is_eligible_event(event, &pwd)) {
                let _ = sender.try_send(event);
            }
        },
        Config::default(),
    )?;
    watcher.watch(Path::new("."), RecursiveMode::Recursive)?;

    let instantiation_args = Instantiation {
        constructor,
        args: args.as_deref(),
        suri: suri.as_deref(),
        url: url.as_deref(),
        gas: *gas,
        proof_size: *proof_size,
    };

    let mut thread_rng = thread_rng();

    while receiver.recv().await.is_some() {
        loop {
            // Wait for any additional changes before starting the project build process.
            tokio::time::sleep(Duration::from_secs(2)).await;

            match receiver.try_recv() {
                Ok(_) => {
                    continue;
                }
                Err(TryRecvError::Empty) => {
                    let (address, metadata) = match build_and_deploy(
                        &cargo,
                        &instantiation_args,
                        cargo_contract_flags,
                        &progress,
                        thread_rng.gen(),
                    )
                    .await
                    {
                        Ok(val) => val,
                        Err(WatchError::BuildError(BuildError::BuildError)) => {
                            break;
                        }
                        Err(e) => return Err(e),
                    };

                    info_sender.send(Some(ContractInfo {
                        node: url
                            .clone()
                            .unwrap_or_else(|| String::from("ws://127.0.0.1:9944")),
                        address,
                        metadata,
                    }))?;

                    reset_progress(&progress);

                    break;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

/// Check if the provided [`Event`] is eligible to be used as a trigger
/// for project rebuild.
///
/// Project rebuild should occur if there is any modification to a non-hidden file
/// that is not related to Rust build artifacts.
fn is_eligible_event(event: &Event, pwd: &Path) -> bool {
    event
        .paths
        .iter()
        .map(|path| {
            path.canonicalize()
                .map_err(WatchError::from)
                .and_then(|path| {
                    path.strip_prefix(pwd)
                        .map(ToOwned::to_owned)
                        .map_err(Into::into)
                })
        })
        .filter_ok(|path| {
            path.components()
                .next()
                .filter(|component| AsRef::<Path>::as_ref(component).as_os_str() == "target")
                .is_some()
                || path
                    .components()
                    .any(|component| AsRef::<Path>::as_ref(&component).starts_with("."))
        })
        .next()
        .is_none()
}

/// Build and deploy a contract locally.
async fn build_and_deploy(
    cargo: &Path,
    instantiation_args: &Instantiation<'_>,
    cargo_contract_flags: &[String],
    progress: &ProgressBar,
    salt: u64,
) -> Result<(String, serde_json::Value), WatchError> {
    progress.set_message("Building...");
    progress.disable_steady_tick();

    let build_result = build_locally(cargo, false).await?;

    let metadata_file = BufReader::new(File::open(build_result.metadata_result.dest_metadata)?);
    let metadata: serde_json::Value = serde_json::from_reader(metadata_file)?;

    progress.set_message("Deploying...");

    let address =
        instantiate_contract(cargo, instantiation_args, cargo_contract_flags, None, salt).await?;

    Ok((address, metadata))
}

/// Reset progress bar to default message and restore periodic ticks.
fn reset_progress(progress: &ProgressBar) {
    progress.enable_steady_tick(Duration::from_millis(150));
    progress.set_message("Watching for changes...");
}
