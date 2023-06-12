use std::{convert::identity, io, sync::Arc, time::Duration};

use bollard::Docker;
use common::{config, hash, s3};
use db::{
    build_session, build_session_token, code,
    sea_query::{LockBehavior, LockType, OnConflict},
    source_code, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    QuerySelect, TransactionErrorExt, TransactionTrait,
};
use derive_more::{Display, Error, From};
use futures_util::{pin_mut, StreamExt};
use itertools::Itertools;
use tokio::{sync::mpsc::UnboundedSender, time::timeout};
use tracing::{error, info, instrument};

use crate::{
    log_collector::LogEntry,
    process::{container::Container, volume::Volume},
};

use super::{
    container::{ContainerRemoveError, DownloadFromContainerError},
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

    /// Docker-related error.
    DockerError(bollard::errors::Error),

    /// IO-related error.
    IoError(io::Error),

    /// S3 storage-related error.
    S3Error(s3::Error),

    /// Volume-related error.
    VolumeError(VolumeError),

    /// Unable to acquire a [build session token](db::build_session_token)
    #[display(fmt = "missing build session token")]
    MissingBuildSessionToken,

    /// Unable to find a [source code](db::source_code) related to the current build session.
    #[display(fmt = "missing source code")]
    MissingSourceCode,
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
    docker: Arc<Docker>,
    db: Arc<DatabaseConnection>,
    log_sender: UnboundedSender<LogEntry>,
) {
    loop {
        let outcome = db
            .transaction::<_, _, WorkerError>(|txn| {
                let builder_config = builder_config.clone();
                let storage_config = storage_config.clone();
                let docker = docker.clone();
                let log_sender = log_sender.clone();

                Box::pin(async move {
                    let mut session_query = build_session::Entity::find()
                        .select_only()
                        .columns([
                            build_session::Column::Id,
                            build_session::Column::SourceCodeId,
                            build_session::Column::RustcVersion,
                            build_session::Column::CargoContractVersion,
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
                        let archive_hash =
                            source_code::Entity::find_by_id(build_session.source_code_id)
                                .select_only()
                                .column(source_code::Column::ArchiveHash)
                                .into_tuple::<Vec<u8>>()
                                .one(txn)
                                .await?
                                .ok_or(WorkerError::MissingSourceCode)?;

                        let token = build_session_token::Entity::find()
                            .select_only()
                            .column(build_session_token::Column::Token)
                            .filter(
                                build_session_token::Column::BuildSessionId.eq(build_session.id),
                            )
                            .into_tuple::<String>()
                            .one(txn)
                            .await?
                            .ok_or(WorkerError::MissingBuildSessionToken)?;

                        let source_code_url = s3::ConfiguredClient::new(&storage_config)
                            .await
                            .get_source_code(&archive_hash)
                            .await?;

                        let volume =
                            Volume::new(&builder_config.images_path, &builder_config.volume_size)
                                .await?;

                        let container = Container::new(
                            &builder_config,
                            &docker,
                            volume,
                            &token,
                            &build_session.rustc_version,
                            &build_session.cargo_contract_version,
                            source_code_url.uri(),
                        )
                        .await?;

                        let mut wasm_buf = vec![0; builder_config.wasm_size_limit];
                        let mut metadata_buf = vec![0; builder_config.metadata_size_limit];

                        match timeout(
                            Duration::from_secs(builder_config.max_build_duration),
                            handle_session(
                                log_sender,
                                build_session.id,
                                &container,
                                &docker,
                                &mut wasm_buf,
                                &mut metadata_buf,
                            ),
                        )
                        .await
                        .map_err(|_| SessionError::TimedOut)
                        .and_then(identity)
                        {
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
                            Err(err) => {
                                info!(id = %build_session.id, ?err, "build session error");

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

                        if let Err(err) = container.remove(&docker).await {
                            error!(?err, "unable to delete container");
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
    /// Docker-related error.
    DockerError(bollard::errors::Error),

    /// Volume-related error.
    VolumeError(VolumeError),

    /// Unable to remove the container.
    ContainerRemoveError(ContainerRemoveError),

    /// Unable to download files from the container.
    DownloadFromContainerError(DownloadFromContainerError),

    /// Container finished its execution with a status code.
    #[display(fmt = "container exited with status code {}", _0)]
    ContainerExited(#[error(not(source))] i64),

    /// Container ran out of time to complete the build.
    #[display(fmt = "container timed out")]
    TimedOut,
}

/// Handle a single build session.
///
/// Returns a tuple of WASM blob bytes and JSON metadata bytes if the
/// contract build is successful, [`SessionError`] otherwise.
async fn handle_session<'a>(
    log_sender: UnboundedSender<LogEntry>,
    build_session_id: i64,
    container: &Container,
    docker: &Docker,
    wasm_buf: &'a mut [u8],
    metadata_buf: &'a mut [u8],
) -> Result<(&'a [u8], &'a [u8]), SessionError> {
    let mut events = container.events(docker);

    let logs = tokio_stream::StreamExt::chunks_timeout(
        container.logs(docker).await?,
        10,
        Duration::from_secs(3),
    );

    pin_mut!(logs);

    loop {
        tokio::select! {
            Some(chunk) = logs.next() => {
                let text = chunk.into_iter()
                    .try_collect::<_, Vec<_>, _>()?
                    .into_iter()
                    .join("");

                let result = log_sender.send(LogEntry {
                    build_session_id,
                    text
                });

                if let Err(e) = result {
                    error!(%e, "unable to send log entry")
                }
            },
            Some(event) = events.next() => match event {
                Ok(_) => {
                    let wasm = container.wasm_file(docker, wasm_buf).await?;
                    let metadata = container.metadata_file(docker, metadata_buf).await?;

                    return Ok((wasm, metadata));
                },
                Err(bollard::errors::Error::DockerContainerWaitError { code, .. }) => {
                    return Err(SessionError::ContainerExited(code));
                },
                Err(err) => return Err(err.into())
            }
        }
    }
}
