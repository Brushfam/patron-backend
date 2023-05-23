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

#[derive(Debug, Display, Error, From)]
pub enum ContainerRemoveError {
    Docker(Error),
    Volume(VolumeError),
}

#[derive(Debug, Display, Error, From)]
pub enum DownloadFromContainerError {
    Docker(Error),
    Io(io::Error),

    #[display(fmt = "file size limit exceeded")]
    FileSizeLimitExceeded,

    #[display(fmt = "file not found")]
    FileNotFound,
}

pub struct Container {
    id: String,
    volume: Volume,
}

impl Container {
    pub async fn new<U: fmt::Display>(
        config: &config::Builder,
        client: &Docker,
        volume: Volume,
        build_session_token: &str,
        rust_version: &str,
        cargo_contract_version: &str,
        source_code_url: U,
    ) -> Result<Self, Error> {
        let host_config = HostConfig {
            cap_add: Some(vec![String::from("DAC_OVERRIDE")]),
            cap_drop: Some(vec![String::from("ALL")]),
            memory: Some(config.memory_limit),
            memory_swap: Some(config.memory_swap_limit),
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
                    name: build_session_token,
                    ..Default::default()
                }),
                Config {
                    image: Some("ink-builder"),
                    env: Some(vec![
                        &format!("SOURCE_CODE_URL={source_code_url}"),
                        &format!("CARGO_CONTRACT_VERSION={cargo_contract_version}"),
                        &format!("RUST_VERSION={rust_version}"),
                        &format!("BUILD_SESSION_TOKEN={build_session_token}"),
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

    pub async fn wasm_file<'a>(
        &self,
        client: &Docker,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(client, "/root/artifacts/ink/main.wasm", buf)
            .await
    }

    pub async fn metadata_file<'a>(
        &self,
        client: &Docker,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(client, "/root/artifacts/ink/main.json", buf)
            .await
    }

    pub fn events(
        &self,
        client: &Docker,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>> {
        client.wait_container::<String>(&self.id, None)
    }

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
