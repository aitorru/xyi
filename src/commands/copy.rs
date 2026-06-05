use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, BufWriter, Read, Write},
    sync::Mutex,
};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use seahash::hash;

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
    let files = scan_for_files_to_copy(from_path.clone(), &folder_bar).await;

    // Create a new progress bar for copying the files
    let transfering_bar = bar.add(ProgressBar::new(files.len() as u64));
    transfering_bar.set_style(style_files_transfering.clone());

    // Create a log writer
    let log_writer = match log_path {
        Some(path) => {
            let file = OpenOptions::new()
                .write(true)
                .append(true)
                .open(path)
                .unwrap();
            Some(Mutex::new(file))
        }
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

async fn scan_for_files_to_copy(
    entry_path: std::path::PathBuf,
    scanning_bar: &ProgressBar,
) -> Vec<FilesSource> {
    let mut files: Vec<FilesSource> = vec![];
    let mut folders: Vec<FoldersSource> = vec![];
    // Scan all the files and folders in the current directory
    for entry in std::fs::read_dir(entry_path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name().into_string().unwrap();
        scanning_bar.set_message(format!("Reading {}", name));
        if path.is_dir() {
            folders.push(FoldersSource { path });
        } else {
            files.push(FilesSource { path, name });
        }
    }
    loop {
        if folders.len() == 0 {
            break;
        }
        let mut folders_to_travel: Vec<FoldersSource> = vec![];
        for folder in folders.clone() {
            for entry in std::fs::read_dir(&folder.path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let name = entry.file_name().into_string().unwrap();
                scanning_bar.set_message(format!("Reading {}", name));
                if path.is_dir() {
                    folders_to_travel.push(FoldersSource { path });
                } else {
                    files.push(FilesSource { path, name });
                }
            }
        }
        // Empty the folders vector
        folders.clear();
        // Add the new folders to the folders vector
        folders.append(&mut folders_to_travel);
    }
    // End the progress bar
    scanning_bar.finish_with_message(format!("Done reading {} files", files.len()));
    files
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
    // If the starting_path ends with a separator remove it
    let starting_path = strip_trailing_separator(starting_path.to_str().unwrap()).to_string();
    // Split the to_path and make sure all the folders exist, if not create them
    for folder in from_path
        .parent()
        .unwrap()
        .to_str()
        .unwrap()
        .replace(&starting_path, "")
        .split('\\')
        .collect::<Vec<&str>>()
    {
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
                    from_path.to_str().unwrap(),
                    to_path.to_str().unwrap(),
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

/// Remove a single trailing path separator (`/` or `\`) from `path`, if present.
fn strip_trailing_separator(path: &str) -> &str {
    path.strip_suffix(['/', '\\']).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_unix_separator() {
        assert_eq!(strip_trailing_separator("/home/user/"), "/home/user");
    }

    #[test]
    fn strips_windows_separator() {
        assert_eq!(strip_trailing_separator("C:\\files\\"), "C:\\files");
    }

    #[test]
    fn leaves_path_without_separator_untouched() {
        assert_eq!(strip_trailing_separator("/home/user"), "/home/user");
    }
}
