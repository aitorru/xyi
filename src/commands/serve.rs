use axum::http::Response;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use axum::{
    body::StreamBody,
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, StatusCode},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    println,
    sync::{Arc, Mutex},
};
use tokio_util::io::ReaderStream;
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

async fn download_file(query: Query<DownloadQuery>) -> impl IntoResponse {
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
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {}", err))),
    };
    let content_size = file.metadata().await.unwrap().len().to_string();
    // convert the `AsyncRead` into a `Stream`
    let stream = ReaderStream::new(file);
    // convert the `Stream` into an `axum::body::HttpBody`
    let body = StreamBody::new(stream);

    let headers = [
        (
            header::CONTENT_TYPE,
            "application/octet-stream; charset=utf-8",
        ),
        (
            header::CONTENT_DISPOSITION,
            &format!(
                "attachment; filename=\"{}\"",
                PathBuf::from(&path).file_name().unwrap().to_str().unwrap()
            ),
        ),
        (header::CONTENT_LENGTH, &content_size),
    ];

    Ok((headers, body).into_response())
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
