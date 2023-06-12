use std::{
    env::current_dir,
    ffi::OsStr,
    fs::File,
    io::{self, Seek, Write},
    path::{Path, StripPrefixError},
};

use derive_more::{Display, Error, From};
use indicatif::ProgressBar;
use walkdir::{DirEntry, WalkDir};
use zip::{write::FileOptions, ZipWriter};

/// Errors that may occur during the archive creation process.
#[derive(Debug, Display, From, Error)]
pub(crate) enum ArchiverError {
    /// [`zip`]-crate specific error.
    Zip(zip::result::ZipError),

    /// [`walkdir`]-crate specific error.
    WalkDir(walkdir::Error),

    /// IO error.
    Io(io::Error),

    /// Unable to strip current directory prefix from path.
    StripPrefix(StripPrefixError),
}

/// Archive the current directory into the provided `file`.
///
/// [`build_zip_archive`] makes use of a [`walk_project_directory`] function,
/// including its file filtering capabilities. See the corresponding documentation
/// for more information on which files and directories are ignored during the packaging
/// process.
pub(crate) fn build_zip_archive<W: Write + Seek>(
    file: W,
    progress: &ProgressBar,
) -> Result<W, ArchiverError> {
    let mut writer = ZipWriter::new(file);

    let current_dir = current_dir()?;
    let mut entries = walk_project_directory(&current_dir);

    while let Some(entry) = entries.next().transpose()? {
        let Some(path) = entry.path().strip_prefix(&current_dir)?.to_str() else {
            progress.println(format!("File {} contains non-unicode symbols in path", entry.path().display()));
            continue;
        };

        if !path.is_empty() {
            if entry.file_type().is_dir() {
                writer.add_directory(path, FileOptions::default())?;
            } else if entry.file_type().is_file() {
                writer.start_file(path, FileOptions::default())?;
                io::copy(&mut File::open(path)?, &mut writer)?;
            }
        }
    }

    Ok(writer.finish()?)
}

/// Recursively iterate over the project files and directories while filtering them.
///
/// Returned [`Iterator`] will not yield any files or directories that are named `target`
/// or any hidden files, names of which begin with a dot (`.git`, `.vscode`, etc.).
fn walk_project_directory(dir: &Path) -> impl Iterator<Item = Result<DirEntry, walkdir::Error>> {
    WalkDir::new(dir).into_iter().filter_entry(|entry| {
        entry
            .path()
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|name| *name != "target" && !name.starts_with('.'))
            .is_some()
    })
}
