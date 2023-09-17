use std::{path::PathBuf, sync::Arc, time::Duration};

use bollard::Docker;
use common::{config, hash, s3};
use db::{
    build_session::{self, ProcessedBuildSession},
    build_session_token, code,
    sea_query::{LockBehavior, LockType, OnConflict},
    source_code, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, QueryFilter, QuerySelect, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, StreamExt, TryFutureExt};
use itertools::Itertools;
use normalize_path::NormalizePath;
use tokio::{sync::mpsc::UnboundedSender, time::timeout};
use tracing::{debug, error, instrument};

use crate::{
    log_collector::LogEntry,
    process::{container::Container, volume::Volume},
};

use super::{
    container::{ContainerRemoveError, DownloadFromContainerError, Image},
    volume::VolumeError,
};

/// [`Duration`] between each failed build session fetch attempt.
const UPDATE_PERIOD: Duration = Duration::from_secs(5);

/// Worker errors, which are usually caused by the deployment environment itself.
///
/// Such errors indicate that an error is not constrained to a single build session,
/// and thus must be dealt with by the builder server administrator.
#[derive(Debug, Display, Error, From)]
pub(crate) enum WorkerError {
    /// Database-related error.
    DatabaseError(DbErr),
}

/// Spawn a worker that will handle incoming build sessions.
///
/// [`Future`] returned by this function is meant to be spawned in the background,
/// as it handles new build sessions in a loop, while also attempting to recover
/// from any occuring errors.
///
/// [`Future`]: std::future::Future
#[instrument(skip_all)]
pub(crate) async fn spawn(
    builder_config: Arc<config::Builder>,
    storage_config: Arc<config::Storage>,
    supported_cargo_contract_versions: Arc<Vec<String>>,
    docker: Arc<Docker>,
    db: Arc<DatabaseConnection>,
    log_sender: UnboundedSender<LogEntry>,
) {
    loop {
        let outcome = db
            .transaction::<_, _, WorkerError>(|txn| {
                let builder_config = builder_config.clone();
                let storage_config = storage_config.clone();
                let supported_cargo_contract_versions = supported_cargo_contract_versions.clone();
                let docker = docker.clone();
                let log_sender = log_sender.clone();

                Box::pin(async move {
                    let mut session_query = build_session::Entity::find()
                        .select_only()
                        .columns([
                            build_session::Column::Id,
                            build_session::Column::SourceCodeId,
                            build_session::Column::CargoContractVersion,
                            build_session::Column::ProjectDirectory,
                        ])
                        .filter(build_session::Column::Status.eq(build_session::Status::New));

                    // Skip any locked build sessions to handle the build session
                    // table as a queue.
                    QuerySelect::query(&mut session_query)
                        .lock_with_behavior(LockType::NoKeyUpdate, LockBehavior::SkipLocked);

                    if let Some(build_session) = session_query
                        .into_model::<build_session::ProcessedBuildSession>()
                        .one(txn)
                        .await?
                    {
                        let mut wasm_buf = vec![0; builder_config.wasm_size_limit];
                        let mut metadata_buf = vec![0; builder_config.metadata_size_limit];

                        let val = |wasm_buf, metadata_buf| async {
                            Instance::new(
                                &build_session,
                                &builder_config,
                                &docker,
                                &storage_config,
                                txn,
                            )
                            .unarchive()
                            .await?
                            .build(log_sender, &supported_cargo_contract_versions)
                            .await?
                            .get_files(wasm_buf, metadata_buf)
                            .await
                        };

                        match val(&mut wasm_buf, &mut metadata_buf).await {
                            Ok((wasm, metadata)) => {
                                let code_hash = hash::blake2(wasm);

                                build_session::Entity::update_many()
                                    .filter(build_session::Column::Id.eq(build_session.id))
                                    .col_expr(
                                        build_session::Column::Status,
                                        build_session::Status::Completed.into(),
                                    )
                                    .col_expr(
                                        build_session::Column::CodeHash,
                                        (&code_hash[..]).into(),
                                    )
                                    .col_expr(build_session::Column::Metadata, metadata.into())
                                    .exec(txn)
                                    .await?;

                                code::Entity::insert(code::ActiveModel {
                                    hash: ActiveValue::Set(code_hash.to_vec()),
                                    code: ActiveValue::Set(wasm.to_vec()),
                                })
                                .on_conflict(
                                    OnConflict::column(code::Column::Hash)
                                        .do_nothing()
                                        .to_owned(),
                                )
                                .exec_without_returning(txn)
                                .await?;
                            }
                            Err(_) => {
                                build_session::Entity::update_many()
                                    .filter(build_session::Column::Id.eq(build_session.id))
                                    .col_expr(
                                        build_session::Column::Status,
                                        build_session::Status::Failed.into(),
                                    )
                                    .exec(txn)
                                    .await?;
                            }
                        }

                        Ok(false)
                    } else {
                        Ok(true)
                    }
                })
            })
            .await
            .into_raw_result();

        match outcome {
            Ok(empty) if empty => tokio::time::sleep(UPDATE_PERIOD).await,
            Err(error) => error!(%error, "worker error"),
            _ => {}
        }
    }
}

/// Build session errors, which are constrained down to a single container
/// and are usually caused by an incorrect user input.
#[derive(Debug, Display, Error, From)]
enum SessionError {
    /// Database-related error.
    DatabaseError(DbErr),

    /// Docker-related error.
    DockerError(bollard::errors::Error),

    /// S3 storage-related error.
    S3Error(s3::Error),

    /// Volume-related error.
    VolumeError(VolumeError),

    /// Unable to remove the container.
    ContainerRemoveError(ContainerRemoveError),

    /// Unable to download files from the container.
    DownloadFromContainerError(DownloadFromContainerError),

    /// Unable to acquire a [build session token](db::build_session_token)
    #[display(fmt = "missing build session token")]
    MissingBuildSessionToken,

    /// Unable to find a [source code](db::source_code) related to the current build session.
    #[display(fmt = "missing source code")]
    MissingSourceCode,

    /// Container finished its execution with a status code.
    #[display(fmt = "container exited with status code {}", _0)]
    ContainerExited(#[error(not(source))] i64),

    /// Container ran out of time to complete the build.
    #[display(fmt = "container timed out")]
    TimedOut,

    /// Unsupported cargo-contract version.
    #[display(fmt = "unsupported cargo-contract version")]
    UnsupportedCargoContractVersion,
}

/// Archived build session instance.
struct Instance<'a> {
    /// Inner build session database record.
    build_session: &'a ProcessedBuildSession,
    /// Builder component configuration.
    builder_config: &'a config::Builder,
    /// Docker RPC client.
    docker: &'a Docker,
    /// AWS S3 storage configuration.
    storage_config: &'a config::Storage,
    /// Current database transaction.
    txn: &'a DatabaseTransaction,
}

impl<'a> Instance<'a> {
    /// Create new build session [`Instance`].
    fn new(
        build_session: &'a ProcessedBuildSession,
        builder_config: &'a config::Builder,
        docker: &'a Docker,
        storage_config: &'a config::Storage,
        txn: &'a DatabaseTransaction,
    ) -> Self {
        Instance {
            build_session,
            builder_config,
            docker,
            storage_config,
            txn,
        }
    }

    /// Unarchive user-provided files using a separately launched container instance.
    ///
    /// This method returns [`UnarchivedInstance`], which can be used to start the build process itself.
    #[instrument(skip(self), fields(id = %self.build_session.id), err(level = "info"))]
    async fn unarchive(self) -> Result<UnarchivedInstance<'a>, SessionError> {
        let archive_hash = source_code::Entity::find_by_id(self.build_session.source_code_id)
            .select_only()
            .column(source_code::Column::ArchiveHash)
            .into_tuple::<Vec<u8>>()
            .one(self.txn)
            .await?
            .ok_or(SessionError::MissingSourceCode)?;

        let token = build_session_token::Entity::find()
            .select_only()
            .column(build_session_token::Column::Token)
            .filter(build_session_token::Column::BuildSessionId.eq(self.build_session.id))
            .into_tuple::<String>()
            .one(self.txn)
            .await?
            .ok_or(SessionError::MissingBuildSessionToken)?;

        let source_code_url = s3::ConfiguredClient::new(self.storage_config)
            .await
            .get_source_code(&archive_hash)
            .await?;

        debug!("creating new volume for build session");

        let volume = Volume::new(
            &self.builder_config.images_path,
            &self.builder_config.volume_size,
        )
        .await?;

        debug!("spawning container for the unarchiving process");

        let container = match Container::new(
            self.builder_config,
            self.docker,
            volume,
            &format!("unarchive-{}", self.build_session.id),
            Image::Unarchive,
            Some(vec![
                &format!("BUILD_SESSION_TOKEN={token}"),
                &format!("SOURCE_CODE_URL={}", source_code_url.uri()),
                &format!("API_SERVER_URL={}", self.builder_config.api_server_url),
            ]),
            None,
        )
        .await
        {
            Ok(container) => container,
            Err((err, volume)) => {
                volume.close().await?;
                return Err(err.into());
            }
        };

        let volume = wait_and_remove(container, self.docker, self.builder_config).await?;

        debug!("unarchiving process completed successfully");

        Ok(UnarchivedInstance {
            build_session: self.build_session,
            builder_config: self.builder_config,
            docker: self.docker,
            volume,
        })
    }
}

/// Build session instance with unarchived user files.
struct UnarchivedInstance<'a> {
    /// Inner build session database record.
    build_session: &'a ProcessedBuildSession,
    /// Builder component configuration.
    builder_config: &'a config::Builder,
    /// Docker RPC client.
    docker: &'a Docker,
    /// Inner volume with unarchived source code.
    volume: Volume,
}

impl<'a> UnarchivedInstance<'a> {
    /// Start build process for the current build session instance.
    #[instrument(skip(self, log_sender, supported_cargo_contract_versions), fields(id = %self.build_session.id), err(level = "info"))]
    pub async fn build(
        self,
        log_sender: UnboundedSender<LogEntry>,
        supported_cargo_contract_versions: &[String],
    ) -> Result<BuiltInstance<'a>, SessionError> {
        debug!("spawning container for building purposes");

        if !supported_cargo_contract_versions.contains(&self.build_session.cargo_contract_version) {
            let result = log_sender
                .send(LogEntry {
                    build_session_id: self.build_session.id,
                    text: String::from("Provided cargo-contract version is not supported.\n"),
                })
                .and_then(|_| {
                    log_sender.send(LogEntry {
                        build_session_id: self.build_session.id,
                        text: format!(
                            "Consider using version {}",
                            supported_cargo_contract_versions.first().expect(
                                "at least one cargo-contract version is expected to be supported"
                            )
                        ),
                    })
                });

            if let Err(e) = result {
                error!(%e, "unable to send log entry")
            }

            return Err(SessionError::UnsupportedCargoContractVersion);
        }

        let normalized_path =
            normalize_working_dir(self.build_session.project_directory.as_deref())
                .display()
                .to_string();

        let container = match Container::new(
            self.builder_config,
            self.docker,
            self.volume,
            &format!("build-session-{}", self.build_session.id),
            Image::Build {
                version: &self.build_session.cargo_contract_version,
            },
            None,
            Some(&normalized_path),
        )
        .await
        {
            Ok(container) => container,
            Err((err, volume)) => {
                volume.close().await?;
                return Err(err.into());
            }
        };

        let volume = handle_session(
            log_sender,
            self.build_session.id,
            container,
            self.docker,
            self.builder_config,
        )
        .await?;

        debug!("container built successfully");

        Ok(BuiltInstance {
            build_session: self.build_session,
            builder_config: self.builder_config,
            docker: self.docker,
            volume,
            normalized_path,
        })
    }
}

/// Build session with WASM and metadata artifacts available
struct BuiltInstance<'a> {
    /// Inner build session database record.
    build_session: &'a ProcessedBuildSession,
    /// Builder component configuration.
    builder_config: &'a config::Builder,
    /// Docker RPC client.
    docker: &'a Docker,
    /// Inner volume with unarchived source code.
    volume: Volume,
    /// Normalized project directory path value.
    normalized_path: String,
}

impl<'a> BuiltInstance<'a> {
    /// Rename artifacts files and write them into the provided buffers.
    ///
    /// This methods returns an [`Err`] if the provided buffers are insufficient in size to write
    /// build artifacts.
    #[instrument(skip(self, wasm_buf, metadata_buf), fields(id = %self.build_session.id), err(level = "info"))]
    async fn get_files<'b>(
        self,
        wasm_buf: &'b mut [u8],
        metadata_buf: &'b mut [u8],
    ) -> Result<(&'b [u8], &'b [u8]), SessionError> {
        debug!("spawning container for file rename purposes");

        let container = match Container::new(
            self.builder_config,
            self.docker,
            self.volume,
            &format!("move-{}", self.build_session.id),
            Image::Move,
            None,
            Some(&self.normalized_path),
        )
        .await
        {
            Ok(container) => container,
            Err((err, volume)) => {
                volume.close().await?;
                return Err(err.into());
            }
        };

        let outcome = wait(&container, self.docker, self.builder_config)
            .and_then(|_| async {
                let wasm = container
                    .wasm_file(self.docker, &self.normalized_path, wasm_buf)
                    .await?;

                let metadata = container
                    .metadata_file(self.docker, &self.normalized_path, metadata_buf)
                    .await?;

                debug!(
                    wasm_size = %wasm.len(),
                    metadata_size = %metadata.len(),
                    "retrieved WASM blob and JSON metadata successfully"
                );

                Ok((wasm, metadata))
            })
            .await;

        container.remove(self.docker).await?.close().await?;

        outcome
    }
}

/// Wait for the provided [`Container`] to finish running.
///
/// This function returns an [`Err`] if container returns non-zero exit code.
async fn wait(
    container: &Container,
    docker: &Docker,
    builder_config: &config::Builder,
) -> Result<(), SessionError> {
    match timeout(
        Duration::from_secs(builder_config.max_build_duration),
        container.events(docker).next(),
    )
    .await
    .map_err(|_| SessionError::TimedOut)?
    {
        Some(Ok(_)) | None => Ok(()),
        Some(Err(bollard::errors::Error::DockerContainerWaitError { code, .. })) => {
            Err(SessionError::ContainerExited(code))
        }
        Some(Err(err)) => Err(err.into()),
    }
}

/// Wait for the provided [`Container`] to finish running and automatically delete it afterwards.
///
/// If an error occurs during the deletion process, this function will automatically attempt to close the backing [`Volume`].
async fn wait_and_remove(
    container: Container,
    docker: &Docker,
    builder_config: &config::Builder,
) -> Result<Volume, SessionError> {
    let outcome = wait(&container, docker, builder_config).await;

    let volume = container.remove(docker).await?;

    if let Err(err) = outcome {
        volume.close().await?;
        Err(err)
    } else {
        Ok(volume)
    }
}

/// Handle a single build session.
///
/// Returns the backing volume with WASM and metadata artifacts, [`SessionError`] otherwise.
async fn handle_session<'a>(
    log_sender: UnboundedSender<LogEntry>,
    build_session_id: i64,
    container: Container,
    docker: &Docker,
    builder_config: &config::Builder,
) -> Result<Volume, SessionError> {
    let logs = tokio_stream::StreamExt::chunks_timeout(
        container.logs(docker).await?,
        10,
        Duration::from_secs(3),
    );

    pin_mut!(logs);

    let wait_future = wait_and_remove(container, docker, builder_config);

    pin_mut!(wait_future);

    loop {
        tokio::select! {
            Some(chunk) = logs.next() => {
                let text = strip_ansi_escapes::strip_str(
                    chunk.into_iter()
                    .try_collect::<_, Vec<_>, _>()?
                    .into_iter()
                    .join("")
                );

                let result = log_sender.send(LogEntry {
                    build_session_id,
                    text
                });

                if let Err(e) = result {
                    error!(%e, "unable to send log entry")
                }
            },
            val = &mut wait_future => {
                return val;
            }
        }
    }
}

/// Convert user-supplied `project_directory` path into a normalized [`PathBuf`] value.
fn normalize_working_dir(project_directory: Option<&str>) -> PathBuf {
    let mut path = PathBuf::from("/contract");

    if let Some(project_directory) = project_directory {
        path.push(project_directory);
    }

    path.normalize()
}
