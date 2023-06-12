//! # Creation process
//!
//! To create such a volume, we first create a [temporary file]
//! inside a directory specified in the [configuration](common::config::Builder).
//!
//! After that, we resize it to the required container volume size with `fallocate`
//! and attempt to format it as an ext4 filesystem using `mkfs.ext4`.
//!
//! If the format process is successful, we create a loop device using `udisksctl`
//! which points to the temporary file that we created previously.
//!
//! Generated loop device path is passed to Docker container for mounting purposes
//! during container instantiation later.
//!
//! # Removal process
//!
//! After the container finished its build process, volumes are meant to be deleted,
//! since they are created for a single build session.
//!
//! To delete a volume, `udisksctl` is used to remove the previously created
//! loop device. After the loop device is removed, we simply remove the temporary
//! file created to handle the filesystem itself.
//!
//! [temporary file]: tempfile::NamedTempFile

use std::{io, path::Path, process::Stdio, str};

use derive_more::{Display, Error, From};
use tempfile::NamedTempFile;
use tokio::process::Command;

/// [`Volume`]-related errors.
#[derive(Debug, Display, Error, From)]
pub enum VolumeError {
    /// IO-related error.
    Io(io::Error),

    /// Unable to call `fallocate` binary.
    #[display(fmt = "unable to run fallocate on the temporary file")]
    Fallocate,

    /// Unable to format the temporary file using `mkfs.ext4`.
    #[display(fmt = "unable to format the temporary file as an ext4 filesystem")]
    Mkfs,

    /// Unable to create loop device using `udisksctl`.
    #[display(fmt = "unable to create the device with udisks")]
    Udisks,
}

/// Isolated container volume.
pub struct Volume {
    /// Loop device path.
    device: String,

    /// ext4-formatted temporary file.
    file: NamedTempFile,
}

impl Volume {
    /// Create new [`Volume`] inside the provided `path` with the provided `size`.
    ///
    /// `size` value must be formatted in a way that is compatible with `fallocate`'s
    /// `-l` flag. See `fallocate(1)` man page for more information.
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

    /// Get underlying loop device path.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Close the current volume.
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

    /// Extract loop device path from `udisksctl` stdout output.
    fn extract_udisks_loop_device(output: &[u8]) -> Option<&str> {
        str::from_utf8(output)
            .ok()?
            .split_ascii_whitespace()
            .last()?
            .strip_suffix('.')
    }
}
