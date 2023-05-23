use std::{io, path::Path, process::Stdio, str};

use derive_more::{Display, Error, From};
use tempfile::NamedTempFile;
use tokio::process::Command;

#[derive(Debug, Display, Error, From)]
pub enum VolumeError {
    Io(io::Error),

    #[display(fmt = "unable to run fallocate on the temporary file")]
    Fallocate,

    #[display(fmt = "unable to format the temporary file as an ext4 filesystem")]
    Mkfs,

    #[display(fmt = "unable to create the device with udisks")]
    Udisks,
}

pub struct Volume {
    device: String,
    file: NamedTempFile,
}

impl Volume {
    pub async fn new(path: &Path, size: &str) -> Result<Self, VolumeError> {
        let file = NamedTempFile::new_in(path)?;

        let fallocate = Command::new("fallocate")
            .args(["-l", size])
            .arg(file.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
            .wait()
            .await?;

        if !fallocate.success() {
            return Err(VolumeError::Fallocate);
        }

        let mkfs = Command::new("mkfs.ext4")
            .arg(file.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
            .wait()
            .await?;

        if !mkfs.success() {
            return Err(VolumeError::Mkfs);
        }

        let udisks_output = Command::new("udisksctl")
            .args(["loop-setup", "--no-user-interaction", "-f"])
            .arg(file.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .wait_with_output()
            .await?;

        if !udisks_output.status.success() {
            return Err(VolumeError::Udisks);
        }

        let device = Self::extract_udisks_loop_device(&udisks_output.stdout)
            .ok_or(VolumeError::Udisks)?
            .to_string();

        Ok(Self { device, file })
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub async fn close(self) -> Result<(), VolumeError> {
        let loop_device_removal = Command::new("udisksctl")
            .args(["loop-delete", "--no-user-interaction", "-b"])
            .arg(self.device)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?
            .wait()
            .await?;

        if !loop_device_removal.success() {
            return Err(VolumeError::Udisks);
        }

        self.file.close()?;

        Ok(())
    }

    fn extract_udisks_loop_device(output: &[u8]) -> Option<&str> {
        str::from_utf8(output)
            .ok()?
            .split_ascii_whitespace()
            .last()?
            .strip_suffix('.')
    }
}
