use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::AsyncWriteExt;

pub async fn entry(url: &str, to: Option<&String>) {
    // Download the file from the url as a stream
    let response = match reqwest::get(url).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Server error: {}", error);
            return;
        }
    };

    let file_name = match to {
        Some(to) => to.to_string(),
        None => match response.headers().get("content-disposition") {
            Some(content_disposition) => content_disposition
                .to_str()
                .unwrap()
                .split("filename=")
                .collect::<Vec<&str>>()[1]
                .replace("\"", ""),
            None => {
                // If the url ends with a slash, remove it
                let url = if url.ends_with("/") {
                    let mut url = url.to_string();
                    url.pop().unwrap();
                    url
                } else {
                    url.to_string()
                };
                let url_split = url.split("/").collect::<Vec<&str>>();
                url_split[url_split.len() - 1].to_string()
            }
        },
    };

    // Save the file to the current directory
    let mut file = match tokio::fs::File::create(&file_name).await {
        Ok(file) => file,
        Err(error) => {
            eprintln!("Error creating file: {}", error);
            return;
        }
    };
    // Get the file size from the headers
    let file_size = match response.headers().get("content-length") {
        Some(content_length) => content_length.to_str().unwrap().parse::<u64>().unwrap(),
        None => 0,
    };

    // Create a progress bar. If the file size is 0, then the progress bar will be indeterminate
    let pb = match file_size {
        0 => ProgressBar::new_spinner(),
        _ => ProgressBar::new(file_size),
    };
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar}] {bytes}/{total_bytes} ({eta_precise})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    // Create a stream from the response
    let mut stream = response.bytes_stream();

    // Create the file
    match tokio::fs::File::create(&file_name).await {
        Ok(file) => file,
        Err(error) => {
            eprintln!("Error creating file: {}", error);
            return;
        }
    };

    // Read the stream
    while let Some(data) = stream.next().await {
        let data = match data {
            Ok(data) => data,
            Err(error) => {
                eprintln!("Error reading stream: {}", error);
                return;
            }
        };
        // Write the data to the file
        match file.write_all(&data).await {
            Ok(_) => {}
            Err(error) => {
                eprintln!("Error writing to file: {}", error);
                return;
            }
        };
        // Increment the progress bar
        pb.inc(data.len() as u64);
    }

    // Finish the progress bar
    pb.finish_with_message(format!("Downloaded {} to {}", url, file_name));
}
