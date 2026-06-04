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
        None => match response
            .headers()
            .get("content-disposition")
            .and_then(|value| value.to_str().ok())
            .and_then(file_name_from_content_disposition)
        {
            Some(name) => name,
            None => file_name_from_url(url),
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

/// Derive a file name from the last path segment of a URL, ignoring a single
/// trailing slash.
fn file_name_from_url(url: &str) -> String {
    let url = url.strip_suffix('/').unwrap_or(url);
    match url.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => url.to_string(),
    }
}

/// Extract the file name from a `Content-Disposition` header value, if it
/// contains a `filename=` parameter.
fn file_name_from_content_disposition(value: &str) -> Option<String> {
    value
        .split("filename=")
        .nth(1)
        .map(|name| name.trim().replace('\"', ""))
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_from_plain_url() {
        assert_eq!(
            file_name_from_url("https://example.com/file.zip"),
            "file.zip"
        );
    }

    #[test]
    fn file_name_from_url_with_trailing_slash() {
        assert_eq!(
            file_name_from_url("https://example.com/path/archive.tar.gz/"),
            "archive.tar.gz"
        );
    }

    #[test]
    fn file_name_from_url_without_path() {
        assert_eq!(file_name_from_url("https://example.com"), "example.com");
    }

    #[test]
    fn content_disposition_with_filename() {
        assert_eq!(
            file_name_from_content_disposition("attachment; filename=\"report.pdf\""),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn content_disposition_without_quotes() {
        assert_eq!(
            file_name_from_content_disposition("attachment; filename=report.pdf"),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn content_disposition_without_filename() {
        assert_eq!(file_name_from_content_disposition("inline"), None);
    }
}
