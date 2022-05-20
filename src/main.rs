use std::{net::SocketAddr, sync::{Mutex, Arc}, collections::HashSet};

use axum::{
    Router,
    routing::{get_service, MethodRouter, get},
    http::StatusCode,
    extract::{ws::{WebSocket, Message}, WebSocketUpgrade},
    response::IntoResponse, Extension
};
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use futures::{sink::SinkExt, stream::StreamExt};

// The entire state of the server
struct State {
    // A thread safe set of all users
    users: Mutex<HashSet<String>>,
    // Used to transmit new messages to all users
    tx: broadcast::Sender<String>,
}

type SharedState = Arc<State>;

#[tokio::main]
async fn main() {
    // Host address
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Start server
    axum::Server::bind(&addr)
        .serve(app().into_make_service())
        .await
        .unwrap()
}

fn app() -> Router {
    let users = Mutex::new(HashSet::new());
    let (tx, _) = broadcast::channel(100);

    let state = Arc::new(State { users, tx });

    Router::new()
        .route("/ws", get(handle_upgrade))
        // Serve static a file by default if no route matches
        .fallback(static_dir_service())
        .layer(Extension(state))
}

// Handles serving static files
fn static_dir_service() -> MethodRouter {
    get_service(ServeDir::new("static/")
        .append_index_html_on_directories(true))
        .handle_error(|err: std::io::Error| async move {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Unhandled internal error: {}", err),
            )
        })
}

async fn handle_upgrade(
    ws: WebSocketUpgrade,
    Extension(state): Extension<SharedState>
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: SharedState) {
    let (mut user_tx, mut user_rx) = socket.split();

    let mut username = String::new();
    // Loop over socket stream until we find a text message
    while let Some(Ok(msg)) = user_rx.next().await {
        if let Message::Text(name) = msg {
            {
                // Accquire lock on users set
                let mut users = state.users.lock().unwrap();

                // If the username does not already exist, add it
                if !users.contains(&name) {
                    username.push_str(&name);
                    users.insert(name);
                    break;
                }
            } // Drop lock

            // If the username is already taken, alert the user
            // Use `let _ =` to throw away a value we don't care about
            let _ = user_tx
                .send(Message::Text(format!("Username `{name}` is already taken")))
                .await;

            return;
        }
    }

    // Subscribe to the message channel
    let mut rx = state.tx.subscribe();

    // Send join message
    let _ = state.tx.send(format!("{username} has joined."));

    // Forward all messages to user
    // `tokio::spawn` creates an asynchronous task
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // End loop if a websocket error occurs
            if user_tx.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Broadcast every message user sends
    let tx = state.tx.clone();
    let name = username.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = user_rx.next().await {
            let _ = tx.send(format!("{name}: {text}"));
        }
    });

    // If any of the tasks end, stop the other
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    // Send leave message
    let _ = state.tx.send(format!("{username} left"));

    // Remove user from set
    state.users.lock().unwrap().remove(&username);
}