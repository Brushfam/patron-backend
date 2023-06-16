use std::{
    collections::HashMap,
    fmt,
    io::{self, Cursor, Read, Write},
};

use bollard::{
    container::{
        AttachContainerOptions, Config, CreateContainerOptions, DownloadFromContainerOptions,
        LogOutput, RemoveContainerOptions,
    },
    errors::Error,
    service::MountTypeEnum,
    service::{
        ContainerWaitResponse, HostConfig, Mount, MountVolumeOptions,
        MountVolumeOptionsDriverConfig,
    },
    Docker,
};
use common::config;
use derive_more::{Display, Error, From};
use futures_util::{Stream, TryStreamExt};

use crate::process::volume::{Volume, VolumeError};

/// Errors that may occur during container removal process.
#[derive(Debug, Display, Error, From)]
pub enum ContainerRemoveError {
    /// Docker-related error.
    Docker(Error),

    /// Volume-related error.
    Volume(VolumeError),
}

/// Errors that may occur during an attempt to download a file from container's filesystem.
#[derive(Debug, Display, Error, From)]
pub enum DownloadFromContainerError {
    /// Docker-related error.
    Docker(Error),

    /// IO-related error.
    Io(io::Error),

    /// Unable to fill the byte buffer with the requested file.
    #[display(fmt = "file size limit exceeded")]
    FileSizeLimitExceeded,

    /// The requested file was not found.
    #[display(fmt = "file not found")]
    FileNotFound,
}

/// A single running Docker container instance.
pub struct Container {
    /// Docker-specific container identifier.
    id: String,

    /// Related volume.
    volume: Volume,
}

/// Container environment variables.
pub struct Environment<'a, U: fmt::Display> {
    /// Build session file upload token.
    pub build_session_token: &'a str,

    /// Rust toolchain version used to build the contract.
    pub rustc_version: &'a str,

    /// `cargo-contract` version used to build the contract.
    pub cargo_contract_version: &'a str,

    /// S3 pre-signed URL to the source code archive.
    pub source_code_url: U,

    /// API server URL used to upload the source code archive contents.
    pub api_server_url: &'a str,
}

impl Container {
    /// Spawn new Docker container with the provided configuration.
    pub async fn new<U: fmt::Display>(
        config: &config::Builder,
        client: &Docker,
        volume: Volume,
        env: Environment<'_, U>,
    ) -> Result<Self, Error> {
        // Attempt to isolate container as much as possible.
        //
        // The provided container configuration should protect
        // the build process from using any unnecessary capabilities,
        // stop the container in case if too many processes are spawned
        // (this may occur during archive unpacking).
        let host_config = HostConfig {
            cap_add: Some(vec![String::from("DAC_OVERRIDE")]),
            cap_drop: Some(vec![String::from("ALL")]),
            memory: Some(config.memory_limit),
            memory_swap: Some(config.memory_swap_limit),
            // Mount the passed volume as a home directory of a root user.
            mounts: Some(vec![Mount {
                target: Some(String::from("/root")),
                typ: Some(MountTypeEnum::VOLUME),
                volume_options: Some(MountVolumeOptions {
                    driver_config: Some(MountVolumeOptionsDriverConfig {
                        name: Some(String::from("local")),
                        options: Some(HashMap::from([
                            (String::from("device"), volume.device().to_string()),
                            (String::from("type"), String::from("ext4")),
                        ])),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }]),
            pids_limit: Some(768),
            security_opt: Some(vec![String::from("no-new-privileges")]),
            ..Default::default()
        };

        let container = client
            .create_container(
                Some(CreateContainerOptions {
                    name: env.build_session_token,
                    ..Default::default()
                }),
                Config {
                    image: Some("ink-builder"),
                    // Pass information about the current build session to container
                    env: Some(vec![
                        &format!("SOURCE_CODE_URL={}", env.source_code_url),
                        &format!("CARGO_CONTRACT_VERSION={}", env.cargo_contract_version),
                        &format!("RUST_VERSION={}", env.rustc_version),
                        &format!("BUILD_SESSION_TOKEN={}", env.build_session_token),
                        &format!("API_SERVER_URL={}", env.api_server_url),
                    ]),
                    host_config: Some(host_config),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await?;

        client
            .start_container::<String>(&container.id, None)
            .await?;

        Ok(Self {
            id: container.id,
            volume,
        })
    }

    /// Get a [`Stream`] of logs from the current Docker container.
    pub async fn logs(
        &self,
        client: &Docker,
    ) -> Result<impl Stream<Item = Result<LogOutput, Error>>, Error> {
        let raw = client
            .attach_container::<String>(
                &self.id,
                Some(AttachContainerOptions {
                    stdout: Some(true),
                    stderr: Some(true),
                    stream: Some(true),
                    logs: Some(true),
                    ..Default::default()
                }),
            )
            .await?;

        Ok(raw.output)
    }

    /// Get WASM blob of an ink! smart contract from the container's filesystem.
    ///
    /// Provided `buf` slice can be used to limit the WASM blob size.
    pub async fn wasm_file<'a>(
        &self,
        client: &Docker,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(client, "/root/artifacts/ink/main.wasm", buf)
            .await
    }

    /// Get JSON metadata of an ink! smart contract from the container's filesystem.
    ///
    /// Provided `buf` slice can be used to limit the JSON metadata size.
    pub async fn metadata_file<'a>(
        &self,
        client: &Docker,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(client, "/root/artifacts/ink/main.json", buf)
            .await
    }

    /// Get a [`Stream`] of the current Docker container process events.
    pub fn events(
        &self,
        client: &Docker,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>> {
        client.wait_container::<String>(&self.id, None)
    }

    /// Remove the current Docker container and close the related [`Volume`].
    pub async fn remove(self, client: &Docker) -> Result<(), ContainerRemoveError> {
        client
            .remove_container(
                &self.id,
                Some(RemoveContainerOptions {
                    v: true,
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        self.volume.close().await?;

        Ok(())
    }

    /// Download a file from the container's filesystem to the provided buffer.
    ///
    /// Since Docker wraps downloaded files into a `tar` archive, we re-use the same buffer
    /// to unarchive the downloaded file.
    ///
    /// To ensure that you access only the file's bytes (and not the `tar` archive's bytes)
    /// you can use the slice returned from this function.
    async fn download_from_container_to_buf<'a>(
        &self,
        client: &Docker,
        path: &str,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        let mut cursor = Cursor::new(buf);

        let mut stream =
            client.download_from_container(&self.id, Some(DownloadFromContainerOptions { path }));

        while let Some(chunk) = stream.try_next().await? {
            cursor
                .write(&chunk)
                .map_err(|_| DownloadFromContainerError::FileSizeLimitExceeded)?;
        }

        let position = cursor.position() as usize;

        // Re-use the same buffer to store both archived and unarchived files.
        let (archive, file_buf) = cursor.into_inner().split_at_mut(position);

        let file_size = tar::Archive::new(&*archive)
            .entries()?
            .next()
            .ok_or(DownloadFromContainerError::FileNotFound)??
            .read(file_buf)?;

        Ok(&file_buf[..file_size])
    }
}
