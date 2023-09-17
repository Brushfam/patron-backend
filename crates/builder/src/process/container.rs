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
    image::{CreateImageOptions, ListImagesOptions},
    service::{
        ContainerWaitResponse, HostConfig, Mount, MountTypeEnum, MountVolumeOptions,
        MountVolumeOptionsDriverConfig,
    },
    Docker,
};
use common::config;
use derive_more::{Display, Error, From};
use futures_util::{Stream, TryStreamExt};
use tracing::info;

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

/// Supported container images.
pub enum Image<'a> {
    /// Unarchive image, produced using Nix.
    Unarchive,

    /// Build image, automatically downloaded from Docker registry.
    Build {
        /// `cargo-contract` version to use during image download process.
        version: &'a str,
    },

    /// Artifact rename image, produced using Nix.
    Move,
}

impl<'a> fmt::Display for Image<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Image::Unarchive => write!(f, "stage-unarchive"),
            Image::Build { version } => write!(f, "paritytech/contracts-verifiable:{version}"),
            Image::Move => write!(f, "stage-move"),
        }
    }
}

/// A single running Docker container instance.
pub struct Container {
    /// Docker-specific container identifier.
    id: String,

    /// Related volume.
    volume: Volume,
}

impl Container {
    /// Spawn new Docker container with the provided configuration.
    pub async fn new(
        config: &config::Builder,
        client: &Docker,
        volume: Volume,
        name: &str,
        image: Image<'_>,
        env: Option<Vec<&str>>,
        working_dir: Option<&str>,
    ) -> Result<Self, (Error, Volume)> {
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
                target: Some(String::from("/contract")),
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

        let image_str = image.to_string();

        let cmd = if let Image::Build { .. } = image {
            if let Err(err) = Self::ensure_image_exists(client, &image_str).await {
                return Err((err, volume));
            }

            Some(vec!["build", "--release"])
        } else {
            None
        };

        dbg!(working_dir);

        let container = match client
            .create_container(
                Some(CreateContainerOptions {
                    name,
                    platform: Some("linux/amd64"),
                }),
                Config {
                    image: Some(&*image_str),
                    cmd,
                    env,
                    host_config: Some(host_config),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    working_dir,
                    ..Default::default()
                },
            )
            .await
        {
            Ok(container) => container,
            Err(err) => return Err((err, volume)),
        };

        if let Err(err) = client.start_container::<String>(&container.id, None).await {
            return Err((err, volume));
        }

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
        working_dir: &str,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(
            client,
            &format!("{}/target/ink/main.wasm", working_dir),
            buf,
        )
        .await
    }

    /// Get JSON metadata of an ink! smart contract from the container's filesystem.
    ///
    /// Provided `buf` slice can be used to limit the JSON metadata size.
    pub async fn metadata_file<'a>(
        &self,
        client: &Docker,
        working_dir: &str,
        buf: &'a mut [u8],
    ) -> Result<&'a [u8], DownloadFromContainerError> {
        self.download_from_container_to_buf(
            client,
            &format!("{}/target/ink/main.json", working_dir),
            buf,
        )
        .await
    }

    /// Get a [`Stream`] of the current Docker container process events.
    pub fn events(
        &self,
        client: &Docker,
    ) -> impl Stream<Item = Result<ContainerWaitResponse, Error>> {
        client.wait_container::<String>(&self.id, None)
    }

    /// Remove the current Docker container and retrieve the inner [`Volume`] value.
    pub async fn remove(self, client: &Docker) -> Result<Volume, ContainerRemoveError> {
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

        Ok(self.volume)
    }

    /// Ensure that the image with the provided name exists.
    ///
    /// If it doesn't, an attempt to pull it from Docker registry will be made.
    pub async fn ensure_image_exists(client: &Docker, image: &str) -> Result<(), Error> {
        let list = client
            .list_images(Some(ListImagesOptions {
                filters: HashMap::from([("reference", vec![image])]),
                ..Default::default()
            }))
            .await?;

        if list.is_empty() {
            info!(%image, "downloading missing docker image");

            client
                .create_image(
                    Some(CreateImageOptions {
                        from_image: image,
                        ..Default::default()
                    }),
                    None,
                    None,
                )
                .map_ok(|_| ())
                .try_collect::<()>()
                .await?;
        }

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
