use axum::body::Body;
use axum::body::StreamBody;
use axum::http::{HeaderMap, Request, Response};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
};
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    println,
    sync::{Arc, Mutex},
};
use tokio_util::io::ReaderStream;
use tower::ServiceExt;
#[derive(Clone)]
struct AppState {
    broadcast_sender: tokio::sync::broadcast::Sender<u8>,
    current_dir: Arc<Mutex<std::path::PathBuf>>,
    local_folders: Arc<Mutex<Vec<LocalFolder>>>,
    local_files: Arc<Mutex<Vec<LocalFile>>>,
}
#[derive(Serialize, Deserialize, Debug)]
struct LocalFolder {
    path: std::path::PathBuf,
    name: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct LocalFile {
    path: std::path::PathBuf,
    name: String,
}
#[derive(Serialize, Deserialize, Debug)]
struct WsMessage {
    message_type: String,
    message: String,
}
#[derive(Deserialize)]
struct DownloadQuery {
    path: String,
}

pub async fn entry(port: &str, starting_dir: &str) {
    let (tx, _) = tokio::sync::broadcast::channel::<u8>(1);
    // Create the satate of the app
    let shared_state = AppState {
        broadcast_sender: tx.clone(),
        current_dir: Arc::new(Mutex::new(std::path::PathBuf::from(starting_dir))),
        local_folders: Arc::new(Mutex::new(vec![])),
        local_files: Arc::new(Mutex::new(vec![])),
    };
    tracing_subscriber::fmt::init();
    // Create a task to scan the working directory
    let shared_state_clone = shared_state.clone();
    tokio::task::spawn_blocking(move || loop {
        {
            scan_folder(&shared_state_clone);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        // If 0 users are connected to the websocket server, pause the scan
        while tx.receiver_count() == 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/state", get(ws))
        .route("/update_state", get(ws_change))
        .route("/download", get(download_file))
        .route("/download_zip", get(download_zip))
        .route("/preact.mjs", get(download_preact_mjs))
        .route("/htm.mjs", get(download_htm_mjs))
        .with_state(shared_state);
    let address = format!("0.0.0.0:{}", port);
    println!("Starting server on address http://localhost:{}", port);
    axum::Server::bind(&address.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

fn scan_folder(shared_state_clone: &AppState) {
    let mut folders: Vec<LocalFolder> = vec![];
    let mut files: Vec<LocalFile> = vec![];
    {
        let current_dir_mutex = shared_state_clone.current_dir.lock().unwrap();
        // Scan all the files and folders in the current directory
        for entry in std::fs::read_dir(&*current_dir_mutex).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name().into_string().unwrap();
            if path.is_dir() {
                folders.push(LocalFolder { path, name });
            } else {
                files.push(LocalFile { path, name });
            }
        }
        let mut local_folders_mutex = shared_state_clone.local_folders.lock().unwrap();
        let mut local_files_mutex = shared_state_clone.local_files.lock().unwrap();
        *local_folders_mutex = folders;
        *local_files_mutex = files;
        let _ = shared_state_clone.broadcast_sender.send(1);
    }
}

#[axum::debug_handler]
async fn root() -> impl IntoResponse {
    #[cfg(debug_assertions)]
    let html_contents = std::fs::read_to_string("src/assets/serve/index.html").unwrap();

    #[cfg(not(debug_assertions))]
    let html_contents = include_str!("../assets/serve/index.html");

    // return the html contents
    axum::response::Html(html_contents)
}

async fn ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|ws: WebSocket| async { realtime_file_system(state, ws).await })
}

async fn realtime_file_system(app_state: AppState, mut ws: WebSocket) {
    let mut rx = app_state.broadcast_sender.subscribe();
    // Handle the messages from the broadcast channel
    while let Ok(_) = rx.recv().await {
        let contents: String;
        {
            let local_files = app_state.local_files.lock().unwrap();
            let local_folders = app_state.local_folders.lock().unwrap();
            let current_dir = &*app_state.current_dir.lock().unwrap();
            contents = format!(
                "{{ \"files\": {}, \"folders\": {}, \"current_dir\": {} }}",
                serde_json::to_string(&*local_files).unwrap(),
                serde_json::to_string(&*local_folders).unwrap(),
                serde_json::to_string(&*current_dir).unwrap()
            );
        }
        let _ = ws.send(Message::Text(contents)).await.unwrap();
    }
}

async fn ws_change(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|ws: WebSocket| async { update_state(state, ws).await })
}
async fn update_state(app_state: AppState, mut ws: WebSocket) {
    while let Some(result) = ws.recv().await {
        let message = match result {
            Ok(message) => message,
            Err(_) => break,
        };
        let message = match message {
            Message::Text(message) => message,
            _ => continue,
        };
        let message: WsMessage = match serde_json::from_str(&message) {
            Ok(message) => message,
            Err(_) => continue,
        };
        match message.message_type.as_str() {
            "change_dir" => {
                let mut current_dir = app_state.current_dir.lock().unwrap();
                let new_dir = std::path::PathBuf::from(message.message);
                if new_dir.is_dir() {
                    *current_dir = new_dir;
                }
            }
            "up_dir" => {
                let mut current_dir = app_state.current_dir.lock().unwrap();
                let new_dir = current_dir.parent().unwrap();
                if new_dir.is_dir() {
                    *current_dir = new_dir.to_path_buf();
                }
            }
            _ => {}
        }
    }
}

async fn download_file(
    query: Query<DownloadQuery>,
    _: HeaderMap,
    request: Request<Body>,
) -> impl IntoResponse {
    // `File` implements `AsyncRead`
    let path = match base64::engine::general_purpose::STANDARD.decode(&query.path) {
        Ok(path) => match String::from_utf8(path) {
            Ok(path) => path,
            Err(err) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    format!("Format not correct: {}", err),
                ))
            }
        },
        Err(err) => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("Format not correct: {}", err),
            ))
        }
    };

    // Check if the file exists
    let path = std::path::Path::new(&path);
    if !path.exists() {
        return Err((StatusCode::NOT_FOUND, "File not found".to_owned()));
    }

    let serve_file = tower_http::services::fs::ServeFile::new(&path);
    Ok(serve_file.oneshot(request).await)
}

/// Removes the backing temp file once the response stream is dropped.
struct TempFileGuard(std::path::PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn download_zip(State(state): State<AppState>) -> axum::response::Response {
    // Snapshot the directory currently being served.
    let root = {
        let current_dir = state.current_dir.lock().unwrap();
        current_dir.clone()
    };

    // Stream the archive through a temp file on disk instead of buffering the
    // whole thing in memory: building the zip in a `Vec` would try to allocate
    // gigabytes at once for large trees, which aborts the process (especially on
    // 32-bit targets where the address space cannot satisfy the allocation).
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path =
        std::env::temp_dir().join(format!("xyi-serve-{}-{}.zip", std::process::id(), unique));

    // Building the zip is blocking IO, so keep it off the async executor.
    let build_path = tmp_path.clone();
    let build_root = root.clone();
    let result =
        tokio::task::spawn_blocking(move || zip_directory_to(&build_root, &build_path)).await;

    let name = match result {
        Ok(Ok(name)) => name,
        Ok(Err(err)) => {
            let _ = std::fs::remove_file(&tmp_path);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build zip: {}", err),
            )
                .into_response();
        }
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build zip: {}", err),
            )
                .into_response();
        }
    };

    let file = match tokio::fs::File::open(&tmp_path).await {
        Ok(file) => file,
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to open zip: {}", err),
            )
                .into_response();
        }
    };

    // The guard is moved into the stream closure so the temp file is deleted
    // once the whole response has been sent (or the client disconnects).
    let guard = TempFileGuard(tmp_path);
    let stream = ReaderStream::new(file).map(move |chunk| {
        let _keep = &guard;
        chunk
    });

    Response::builder()
        .header("content-type", "application/zip")
        .header(
            "content-disposition",
            format!("attachment; filename=\"{}.zip\"", name),
        )
        .body(StreamBody::new(stream))
        .unwrap()
        .into_response()
}

/// Recursively zip every file under `root` into `dest`, returning a name
/// suitable for the downloaded file. The archive is written straight to disk so
/// memory usage stays low regardless of how large the tree is.
fn zip_directory_to(root: &std::path::Path, dest: &std::path::Path) -> std::io::Result<String> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, root, root, options)?;
    zip.finish()?;

    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("root")
        .to_owned();

    Ok(name)
}

fn add_dir_to_zip<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    base: &std::path::Path,
    dir: &std::path::Path,
    options: zip::write::FileOptions,
) -> std::io::Result<()> {
    // Be tolerant of unreadable entries: a single inaccessible file or
    // directory (permission denied, file in use, Windows reparse-point
    // junctions like "My Music" inside Documents, ...) must not abort the whole
    // archive. We skip what we cannot read instead of propagating the error.
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        let relative = match path.strip_prefix(base) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        // Zip archives always use forward slashes for entry names.
        let name = relative.to_string_lossy().replace('\\', "/");

        // Use the directory entry's own type so symlinks/junctions are detected
        // without following them (avoids access-denied errors and loops).
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            let _ = zip.add_directory(format!("{}/", name), options);
            add_dir_to_zip(zip, base, &path, options)?;
        } else if file_type.is_file() {
            // Open the file before adding the entry so a failure to read it
            // doesn't leave a half-written entry in the archive.
            let mut file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(_) => continue,
            };
            if zip.start_file(name, options).is_err() {
                continue;
            }
            let _ = std::io::copy(&mut file, zip);
        }
    }
    Ok(())
}

async fn download_preact_mjs() -> impl IntoResponse {
    let js = include_str!("../assets/serve/preact.mjs");
    Response::builder()
        .header("content-type", "application/javascript;charset=utf-8")
        .body(js.to_string())
        .unwrap()
}

async fn download_htm_mjs() -> impl IntoResponse {
    let js = include_str!("../assets/serve/htm.mjs");
    Response::builder()
        .header("content-type", "application/javascript;charset=utf-8")
        .body(js.to_string())
        .unwrap()
}
