use std::io::{BufRead, BufReader, BufWriter, Read, Write};

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

/**
    ### Copy command
    Copy files and directories respecting existing files but comparing them
    ```bash
    xyi.exe copy -f <from> -t <to>
    ```
*/
pub async fn entry(from: String, to: String, force: bool, skip: bool) {
    // Create a new group of progress bars
    let bar = MultiProgress::new();
    let style_scanning = ProgressStyle::with_template("[{elapsed_precise}] {msg}").unwrap();
    let style_files_transfering =
        ProgressStyle::with_template("[{elapsed_precise}] {bar:20} {pos}/{len} {msg}").unwrap();
    let style_files_transfering_files =
        ProgressStyle::with_template("[{elapsed_precise}] {bar:20} {msg}").unwrap();

    let folder_bar = bar.add(ProgressBar::new(1));
    folder_bar.set_style(style_scanning.clone());

    let from_path = std::path::PathBuf::from(from);
    let to_path = std::path::PathBuf::from(to);
    let files = scan_for_files_to_copy(from_path.clone(), &folder_bar).await;

    // Create a new progress bar for copying the files
    let transfering_bar = bar.add(ProgressBar::new(files.len() as u64));
    transfering_bar.set_style(style_files_transfering.clone());

    copy_files(
        files,
        &bar,
        &style_files_transfering_files,
        &transfering_bar,
        from_path,
        to_path,
        force,
        skip,
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

async fn copy_files(
    files: Vec<FilesSource>,
    transfering_bar_multi: &MultiProgress,
    transfering_bar_multi_style: &ProgressStyle,
    transfering_bar_global: &ProgressBar,
    starting_path: std::path::PathBuf,
    to_path: std::path::PathBuf,
    force: bool,
    skip: bool,
) {
    transfering_bar_global.set_message("Copying files");
    files.par_iter().for_each(|file| {
        let from_path = file.path.clone();
        let mut to_path = to_path.clone();
        let name = file.name.clone();
        let path = from_path.clone();
        // If the static_path ends with a / remove it
        let starting_path = if starting_path.to_str().unwrap().ends_with("/")
            || starting_path.to_str().unwrap().ends_with("\\")
        {
            let mut starting_path = starting_path.to_str().unwrap().to_owned();
            // Pop returns the last character, so we need to return the whole starting path without the last character
            starting_path.pop().unwrap();
            starting_path
        } else {
            starting_path.to_str().unwrap().to_string()
        };
        // Split the to_path and make sure all the folders exist, if not create them
        for folder in from_path
            .parent()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(&starting_path, "")
            .split("\\")
            .collect::<Vec<&str>>()
        {
            to_path = to_path.join(folder);
            if !to_path.exists() {
                match std::fs::create_dir(&to_path) {
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
        }
        let to_path = to_path.join(path.file_name().unwrap());

        // Copy he files manually to get the progress bar
        let mut from_file = std::fs::File::open(from_path.clone()).unwrap();
        let mut to_file = std::fs::File::open(&to_path).unwrap();

        let individual_progress = transfering_bar_multi.insert_before(
            transfering_bar_global,
            ProgressBar::new(from_file.metadata().unwrap().len()),
        );
        individual_progress.set_style(transfering_bar_multi_style.clone());
        individual_progress.set_message(format!("Checking {}", name));
        // If the to_path is a file, get the hash and compare them
        if to_path.is_file() && !force {
            if skip {
                individual_progress.finish_with_message(format!("File {} already exists", name));
                transfering_bar_global.inc(1);
                return;
            }
            // Use seahash to compare the hashes of the files
            let mut from_file = std::fs::File::open(from_path.clone()).unwrap();
            let mut to_file = std::fs::File::open(&to_path).unwrap();
            // Read the files in chunks, hash them and compare. If some operation fails, it means that the files are different.
            let mut same_file = true;
            loop {
                let mut buffer_from = [0u8; 1024];
                let mut buffer_to = [0u8; 1024];
                let read_from = from_file.read(&mut buffer_from).unwrap();
                let _ = to_file.read(&mut buffer_to).unwrap();
                if hash(&buffer_from) != hash(&buffer_to) {
                    // Reset the progress bar and continue
                    // individual_progress.println(format!("File {} is different", name));
                    // individual_progress.println(format!("from {:?}", buffer_from));
                    // individual_progress.println(format!("to {:?}", buffer_to));
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
                individual_progress.finish_with_message(format!("File {} already exists", name));
                transfering_bar_global.inc(1);
                return;
            }
        }
        // Reset the progress bar
        individual_progress.set_position(0);
        individual_progress.set_message(format!("Copying {}", name));
        let mut reader_buffer = BufReader::new(&mut from_file);
        let mut writer_buffer = BufWriter::new(&mut to_file);
        loop {
            let bytes_read;
            {
                let buffer = reader_buffer.fill_buf().unwrap();
                if buffer.len() == 0 {
                    break;
                }
                bytes_read = buffer.len();
                writer_buffer.write(buffer).unwrap();
            }
            writer_buffer.flush().unwrap();
            individual_progress.inc(bytes_read as u64);
            reader_buffer.consume(bytes_read);
        }
        individual_progress.finish_with_message(format!("Done copying file {}", name));
        transfering_bar_global.inc(1);
    });
    transfering_bar_global.finish_with_message("Done copying files");
}
