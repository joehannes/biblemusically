// ────────────────────────────────────────────────────────────────
// YouTube Channel Creation Browser Watcher
// Opens YouTube channel creation page and monitors for completion
// ────────────────────────────────────────────────────────────────

use crate::state::AppState;
use bson::{doc, Document};
use serde_json::Value;
use tauri::State;
use tokio::sync::mpsc;
use warp::Filter;
use std::net::SocketAddr;
use tokio::net::TcpListener;

type Res<T> = Result<T, String>;
fn e(err: impl std::fmt::Display) -> String { err.to_string() }

/// Opens the YouTube channel creation page in the system browser
/// and starts a local server to detect when creation is complete.
#[tauri::command]
pub async fn start_channel_creation_watcher(
    state: State<'_, AppState>,
    callback_port: Option<u16>,
) -> Res<Value> {
    use tokio::time::timeout;
    
    // Port for the callback server
    let port = callback_port.unwrap_or(3340);
    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port).parse().map_err(e)?;
    
    // Channel to receive the completion signal with handle
    let (tx, mut rx) = mpsc::channel::<String>(1);
    let tx_filter = warp::any().map(move || tx.clone());
    
    // Simple callback endpoint that accepts POST with handle data
    let callback_route = warp::post()
        .and(warp::path("channel-created"))
        .and(warp::body::json())
        .and(tx_filter.clone())
        .and_then(|payload: Value, tx: mpsc::Sender<String>| async move {
            if let Some(handle) = payload.get("handle").and_then(|h| h.as_str()) {
                let _ = tx.try_send(handle.to_string());
            }
            Ok::<_, std::convert::Infallible>(warp::reply::with_status(
                "OK",
                warp::http::StatusCode::OK,
            ))
        });
    
    // Try to bind to the port
    let tcp = TcpListener::bind(bind_addr).await
        .map_err(|err| format!("Cannot bind callback port {}: {}. Another process may already be using it.", port, err))?;
    let std_listener = tcp.into_std().map_err(e)?;
    
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = warp::serve(callback_route)
        .serve_incoming_with_graceful_shutdown(
            tokio_stream::wrappers::TcpListenerStream::new(
                TcpListener::from_std(std_listener).map_err(e)?
            ),
            async move { shutdown_rx.await.ok(); },
        );
    let server_task = tokio::task::spawn(server);
    
    // Open YouTube channel creation page in system browser
    let creation_url = "https://www.youtube.com/create_channel";
    let _ = open::that(creation_url);
    
    // Wait for callback (5 minutes timeout)
    let result = match timeout(std::time::Duration::from_secs(300), rx.recv()).await {
        Ok(Some(handle)) => {
            // Shut down server gracefully
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            
            Ok(serde_json::json!({
                "ok": true,
                "handle": handle,
                "message": "Channel creation detected"
            }))
        }
        Ok(None) => {
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Err("Callback receiver closed".into())
        }
        Err(_) => {
            let _ = shutdown_tx.send(());
            let _ = server_task.await;
            Err("Timed out waiting for channel creation (5 minutes)".into())
        }
    };
    
    result
}

/// Inject a discovered handle into an existing channel document
#[tauri::command]
pub async fn inject_channel_handle(
    state: State<'_, AppState>,
    channel_id: String,
    handle: String,
) -> Res<Value> {
    let channels_coll = state.db.collection::<Document>("channels");
    
    // Check if channel exists
    let existing = channels_coll
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found".to_string())?;
    
    // Update the channel with the handle (stored in a custom field for now)
    // YouTube API doesn't allow setting handles directly, but we can track it
    channels_coll
        .update_one(
            doc! { "id": &channel_id },
            doc! { "$set": {
                "youtube_handle": &handle,
                "handle_discovered_at": chrono::Utc::now().to_rfc3339(),
            }},
        )
        .await
        .map_err(e)?;
    
    let updated = channels_coll
        .find_one(doc! { "id": &channel_id })
        .await
        .map_err(e)?
        .ok_or_else(|| "Channel not found after update".to_string())?;
    
    Ok(crate::commands::channels::bson_to_value(updated))
}
