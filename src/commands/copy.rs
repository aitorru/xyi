use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    sync::Mutex,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use seahash::hash;

#[derive(serde::Serialize, serde::Deserialize)]
struct FilesSource {
    path: std::path::PathBuf,
    name: String,
}
#[derive(Clone)]
struct FoldersSource {
    path: std::path::PathBuf,
}

/// A failure that happened while copying a single file. Carries the source
/// path together with a human readable description of what went wrong so it
/// can either abort the whole copy or be reported at the end.
#[derive(Debug)]
struct CopyError {
    path: std::path::PathBuf,
    message: String,
}

impl CopyError {
    fn new(path: &std::path::Path, message: impl Into<String>) -> Self {
        CopyError {
            path: path.to_path_buf(),
            message: message.into(),
        }
    }
}

/**
    ### Copy command
    Copy files and directories respecting existing files but comparing them
    ```bash
    xyi.exe copy -f <from> -t <to>
    ```
*/
pub async fn entry(
    from: String,
    to: String,
    force: bool,
    skip: bool,
    hash_check: bool,
    continue_on_error: bool,
    log_path: Option<String>,
    index_path: Option<String>,
) {
    // Create a new group of progress bars
    let bar = MultiProgress::new();
    let style_scanning = ProgressStyle::with_template("[{elapsed}] {msg:40} [{eta}]").unwrap();
    let style_files_transfering =
        ProgressStyle::with_template("[{elapsed}] {bar:20} {pos}/{len} {msg:40} [{eta}]").unwrap();
    let style_files_transfering_files =
        ProgressStyle::with_template("[{elapsed}] {bar:20} {msg:40} [{eta}]").unwrap();

    let folder_bar = bar.add(ProgressBar::new(1));
    folder_bar.set_style(style_scanning.clone());

    let from_path = std::path::PathBuf::from(from);
    let to_path = std::path::PathBuf::from(to);
    // Reuse a cached index when one is available, otherwise scan the source tree
    // and persist the result for the next run. The cache is trusted as-is and is
    // never revalidated against the source, so files added or removed after it
    // was written will not be picked up until the index file is deleted.
    let files = match index_path
        .as_ref()
        .and_then(|path| load_index(path, &folder_bar))
    {
        Some(files) => files,
        None => {
            let files = match scan_for_files_to_copy(from_path.clone(), &folder_bar).await {
                Ok(files) => files,
                Err(error) => {
                    folder_bar.abandon_with_message("Scan aborted due to an error");
                    eprintln!(
                        "\nScan aborted: failed to read '{}': {}",
                        error.path.display(),
                        error.message
                    );
                    std::process::exit(1);
                }
            };
            if let Some(path) = index_path.as_ref() {
                save_index(&files, path);
            }
            files
        }
    };

    // Create a new progress bar for copying the files
    let transfering_bar = bar.add(ProgressBar::new(files.len() as u64));
    transfering_bar.set_style(style_files_transfering.clone());

    // Create a log writer. Logging is a convenience, not a requirement, so a
    // failure to open the log file warns the user and carries on instead of
    // aborting the whole copy.
    let log_writer = match log_path {
        Some(path) => match OpenOptions::new().write(true).append(true).open(&path) {
            Ok(file) => Some(Mutex::new(file)),
            Err(error) => {
                eprintln!(
                    "Warning: could not open log file '{}': {}. Continuing without logging.",
                    path, error
                );
                None
            }
        },
        None => None,
    };

    copy_files(
        files,
        &bar,
        &style_files_transfering_files,
        &transfering_bar,
        from_path,
        to_path,
        force,
        skip,
        hash_check,
        continue_on_error,
        log_writer,
    )
    .await;
}

/// Read a single directory into its files and sub-folders, turning any I/O
/// problem into a [`CopyError`] with a human readable message instead of
/// panicking. File names are decoded lossily so non-UTF-8 names (common on
/// Windows) never abort the scan.
fn read_dir_into(
    dir: &std::path::Path,
    files: &mut Vec<FilesSource>,
    folders: &mut Vec<FoldersSource>,
    scanning_bar: &ProgressBar,
) -> Result<(), CopyError> {
    let entries = std::fs::read_dir(dir).map_err(|error| {
        CopyError::new(dir, format!("could not read source directory: {}", error))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            CopyError::new(dir, format!("could not read a directory entry: {}", error))
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        scanning_bar.set_message(format!("Reading {}", name));
        if path.is_dir() {
            folders.push(FoldersSource { path });
        } else {
            files.push(FilesSource { path, name });
        }
    }
    Ok(())
}

/// Recursively scan `entry_path`, returning the list of files to copy. Any
/// directory that cannot be read aborts the scan with a [`CopyError`] so the
/// caller can report a clear message instead of crashing with a backtrace.
async fn scan_for_files_to_copy(
    entry_path: std::path::PathBuf,
    scanning_bar: &ProgressBar,
) -> Result<Vec<FilesSource>, CopyError> {
    let mut files: Vec<FilesSource> = vec![];
    let mut folders: Vec<FoldersSource> = vec![];
    // Scan all the files and folders in the current directory
    read_dir_into(&entry_path, &mut files, &mut folders, scanning_bar)?;
    loop {
        if folders.is_empty() {
            break;
        }
        let mut folders_to_travel: Vec<FoldersSource> = vec![];
        for folder in folders.clone() {
            read_dir_into(
                &folder.path,
                &mut files,
                &mut folders_to_travel,
                scanning_bar,
            )?;
        }
        // Empty the folders vector
        folders.clear();
        // Add the new folders to the folders vector
        folders.append(&mut folders_to_travel);
    }
    // End the progress bar
    scanning_bar.finish_with_message(format!("Done reading {} files", files.len()));
    Ok(files)
}

/// Load a previously persisted copy index from `path`. Returns `None` when the
/// index cannot be used so the caller can fall back to a fresh scan. A missing
/// file is the normal "first run" case and is silent; anything else (an
/// unreadable or corrupt cache) is reported to the user as a warning before
/// falling back. The cache is trusted as-is and is intentionally not
/// revalidated against the source tree.
fn load_index(path: &str, scanning_bar: &ProgressBar) -> Option<Vec<FilesSource>> {
    if !std::path::Path::new(path).exists() {
        // No cache yet: the scan will create it. Nothing to warn about.
        return None;
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            // Use eprintln! rather than the progress bar so the warning still
            // reaches the user when output is piped or redirected to a file.
            eprintln!(
                "Warning: could not open index cache '{}': {}. Scanning the source instead.",
                path, error
            );
            return None;
        }
    };
    let reader = BufReader::new(file);
    match serde_json::from_reader::<_, Vec<FilesSource>>(reader) {
        Ok(files) => {
            scanning_bar
                .finish_with_message(format!("Loaded {} files from index cache", files.len()));
            Some(files)
        }
        Err(error) => {
            eprintln!(
                "Warning: index cache '{}' is not valid ({}). Scanning the source instead.",
                path, error
            );
            None
        }
    }
}

/// Persist the scanned copy index to `path` so subsequent runs can skip the
/// recursive directory scan via `--index`. A failure to write the cache is
/// non-fatal: the copy has all the information it needs in memory already, so we
/// just warn the user that the cache was not saved and carry on.
fn save_index(files: &[FilesSource], path: &str) {
    let file = match File::create(path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!(
                "Warning: could not create index cache '{}': {}. The index was not saved.",
                path, error
            );
            return;
        }
    };
    let writer = BufWriter::new(file);
    if let Err(error) = serde_json::to_writer(writer, files) {
        eprintln!(
            "Warning: could not write index cache '{}': {}. The index was not saved.",
            path, error
        );
    }
}

#[allow(clippy::too_many_arguments)]
async fn copy_files(
    files: Vec<FilesSource>,
    transfering_bar_multi: &MultiProgress,
    transfering_bar_multi_style: &ProgressStyle,
    transfering_bar_global: &ProgressBar,
    starting_path: std::path::PathBuf,
    to_path: std::path::PathBuf,
    force: bool,
    skip: bool,
    hash_check: bool,
    continue_on_error: bool,
    log_writer: Option<Mutex<File>>,
) {
    transfering_bar_global.set_message("Copying files");

    // Collects the files that failed to copy when running in permissive
    // (`continue_on_error`) mode so they can be reported once everything is done.
    let failures: Mutex<Vec<CopyError>> = Mutex::new(Vec::new());

    // `try_for_each` short-circuits as soon as a closure returns `Err`, which is
    // exactly what we want for the default fail-fast behaviour: the first read or
    // write error stops scheduling any further work. In permissive mode we never
    // return `Err`; instead we stash the error and keep going.
    let result: Result<(), CopyError> =
        files.par_iter().try_for_each(|file| {
            match copy_single_file(
                file,
                transfering_bar_multi,
                transfering_bar_multi_style,
                transfering_bar_global,
                &starting_path,
                &to_path,
                force,
                skip,
                hash_check,
                &log_writer,
            ) {
                Ok(()) => Ok(()),
                Err(error) => {
                    if continue_on_error {
                        failures.lock().unwrap().push(error);
                        Ok(())
                    } else {
                        Err(error)
                    }
                }
            }
        });

    match result {
        // Fail-fast mode hit an error: stop copying everything and bail out with a
        // controlled message instead of an unhandled panic + backtrace.
        Err(error) => {
            transfering_bar_global.abandon_with_message("Copy aborted due to an error");
            eprintln!(
                "\nCopy aborted: failed to copy '{}': {}",
                error.path.display(),
                error.message
            );
            eprintln!(
                "Re-run with --continue-on-error to skip failing files and report them at the end."
            );
            std::process::exit(1);
        }
        Ok(()) => {
            let failures = failures.into_inner().unwrap();
            if failures.is_empty() {
                transfering_bar_global.finish_with_message("Done copying files");
            } else {
                transfering_bar_global.finish_with_message(format!(
                    "Done copying files with {} failure(s)",
                    failures.len()
                ));
                eprintln!("\n{} file(s) failed to copy:", failures.len());
                for failure in &failures {
                    eprintln!("  - '{}': {}", failure.path.display(), failure.message);
                }
                // Signal the failures to the caller / shell.
                std::process::exit(1);
            }
        }
    }
}

/// Copy a single file, returning a [`CopyError`] on the first read/write problem
/// instead of panicking. The caller decides whether such an error aborts the
/// whole operation or is merely collected and reported.
#[allow(clippy::too_many_arguments)]
fn copy_single_file(
    file: &FilesSource,
    transfering_bar_multi: &MultiProgress,
    transfering_bar_multi_style: &ProgressStyle,
    transfering_bar_global: &ProgressBar,
    starting_path: &std::path::Path,
    to_path: &std::path::Path,
    force: bool,
    skip: bool,
    hash_check: bool,
    log_writer: &Option<Mutex<File>>,
) -> Result<(), CopyError> {
    let from_path = file.path.clone();
    let mut to_path = to_path.to_path_buf();
    let name = file.name.clone();
    let path = from_path.clone();
    // Mirror the file's directory structure under the destination. We compute
    // the file's parent directory relative to the source root and walk its
    // components, creating each level as needed. Working on path components
    // (instead of splitting a string on a hard-coded separator) keeps this
    // correct on both Windows (`\`) and Unix (`/`) and avoids panicking on
    // non-UTF-8 paths.
    let parent = from_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let relative = relative_dir(parent, starting_path);
    for folder in relative.components() {
        to_path = to_path.join(folder);
        if !to_path.exists() {
            // Creating an already existing directory races harmlessly between
            // threads, so only a real failure to create a *missing* directory is
            // treated as an error.
            if let Err(error) = std::fs::create_dir(&to_path) {
                if !to_path.exists() {
                    return Err(CopyError::new(
                        &to_path,
                        format!("could not create destination directory: {}", error),
                    ));
                }
            }
        }
    }
    let to_path = to_path.join(path.file_name().unwrap());

    // Open the from file for reading operations
    let mut from_file = std::fs::File::open(from_path.clone())
        .map_err(|error| CopyError::new(&from_path, format!("could not open source: {}", error)))?;

    let source_len = from_file
        .metadata()
        .map_err(|error| {
            CopyError::new(
                &from_path,
                format!("could not read source metadata: {}", error),
            )
        })?
        .len();

    let individual_progress =
        transfering_bar_multi.insert_after(transfering_bar_global, ProgressBar::new(source_len));
    individual_progress.set_style(transfering_bar_multi_style.clone());
    individual_progress.set_message(format!("Checking {}", name));

    // Run the actual copy in an inner closure so the per-file progress bar is
    // always cleared, regardless of whether we succeed or fail.
    let outcome = (|| -> Result<(), CopyError> {
        // If the to_path is a file, get the hash and compare them
        if to_path.is_file() && !force {
            let to_len = std::fs::metadata(&to_path)
                .map_err(|error| {
                    CopyError::new(
                        &to_path,
                        format!("could not read destination metadata: {}", error),
                    )
                })?
                .len();
            if to_len == source_len && !hash_check {
                transfering_bar_global.inc(1);
                return Ok(());
            } else if !hash_check {
                return Ok(());
            }
            if skip {
                transfering_bar_global.inc(1);
                return Ok(());
            }
            // Use seahash to compare the hashes of the files
            let mut from_file = std::fs::File::open(from_path.clone()).map_err(|error| {
                CopyError::new(&from_path, format!("could not open source: {}", error))
            })?;
            let mut to_file = std::fs::File::open(&to_path).map_err(|error| {
                CopyError::new(&to_path, format!("could not open destination: {}", error))
            })?;
            // Read the files in chunks, hash them and compare. If some operation fails, it means that the files are different.
            let mut same_file = true;
            loop {
                let mut buffer_from = [0u8; 1024 * 10];
                let mut buffer_to = [0u8; 1024 * 10];
                let read_from = from_file.read(&mut buffer_from).map_err(|error| {
                    CopyError::new(&from_path, format!("could not read source: {}", error))
                })?;
                let _ = to_file.read(&mut buffer_to).map_err(|error| {
                    CopyError::new(&to_path, format!("could not read destination: {}", error))
                })?;
                if hash(&buffer_from) != hash(&buffer_to) {
                    same_file = false;
                    break;
                }
                // If the buffers are empty, it means that we ended reading the files
                if read_from == 0 {
                    break;
                }
                individual_progress.inc(read_from as u64);
            }
            if same_file {
                transfering_bar_global.inc(1);
                return Ok(());
            }
        }
        // Reset the progress bar
        individual_progress.set_position(0);
        individual_progress.set_message(format!("Copying {}", name));
        let mut reader_buffer = BufReader::new(&mut from_file);
        // At this point we can know for sure that to_path does not exists
        let mut to_file = std::fs::File::create(&to_path).map_err(|error| {
            CopyError::new(&to_path, format!("could not create destination: {}", error))
        })?;
        let mut writer_buffer = BufWriter::new(&mut to_file);
        loop {
            let bytes_read;
            {
                let buffer = reader_buffer.fill_buf().map_err(|error| {
                    CopyError::new(&from_path, format!("could not read source: {}", error))
                })?;
                if buffer.is_empty() {
                    break;
                }
                bytes_read = buffer.len();
                writer_buffer.write_all(buffer).map_err(|error| {
                    CopyError::new(&to_path, format!("could not write destination: {}", error))
                })?;
            }
            writer_buffer.flush().map_err(|error| {
                CopyError::new(&to_path, format!("could not flush destination: {}", error))
            })?;
            individual_progress.inc(bytes_read as u64);
            reader_buffer.consume(bytes_read);
        }
        transfering_bar_global.inc(1);
        // Log the file
        if let Some(log_writer) = log_writer.as_ref() {
            let mut log_writer = log_writer.lock().unwrap();
            let date = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
            // A logging failure should never abort the actual copy, so it is
            // intentionally ignored here.
            let _ = log_writer.write_all(
                format!(
                    "- Copied {} to {} - at {}\n",
                    from_path.display(),
                    to_path.display(),
                    date
                )
                .as_bytes(),
            );
        }
        Ok(())
    })();

    individual_progress.finish_and_clear();
    outcome
}

/// Compute `parent` relative to the source `root` so the directory structure
/// can be recreated under the destination. When `parent` does not live under
/// `root` (for instance when an index built from a different source is reused)
/// the parent is returned unchanged. Comparison is done on whole path
/// components, so a trailing separator on `root` is irrelevant and both Windows
/// and Unix separators are handled natively.
fn relative_dir<'a>(parent: &'a std::path::Path, root: &std::path::Path) -> &'a std::path::Path {
    parent.strip_prefix(root).unwrap_or(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn relative_dir_strips_the_source_root() {
        let parent = PathBuf::from("/data/src/a/b");
        let root = Path::new("/data/src");
        assert_eq!(relative_dir(&parent, root), Path::new("a/b"));
    }

    #[test]
    fn relative_dir_ignores_trailing_separator_on_root() {
        let parent = PathBuf::from("/data/src/a");
        let root = Path::new("/data/src/");
        assert_eq!(relative_dir(&parent, root), Path::new("a"));
    }

    #[test]
    fn relative_dir_is_empty_for_files_in_the_root() {
        let parent = PathBuf::from("/data/src");
        let root = Path::new("/data/src");
        assert_eq!(relative_dir(&parent, root), Path::new(""));
    }

    #[test]
    fn relative_dir_falls_back_when_not_under_root() {
        let parent = PathBuf::from("/somewhere/else");
        let root = Path::new("/data/src");
        assert_eq!(relative_dir(&parent, root), Path::new("/somewhere/else"));
    }
}
