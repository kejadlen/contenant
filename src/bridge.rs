use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router};
use color_eyre::eyre::Result;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::process::Command;
use tracing::info;

pub async fn serve(port: u16, triggers: HashMap<String, String>) -> Result<()> {
    let app = Router::new()
        .route("/triggers/{name}", axum::routing::post(trigger))
        .with_state(Arc::new(triggers));

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "Bridge server listening");

    axum::serve(listener, app).await?;

    Ok(())
}

// --- HTTP handlers ---

#[derive(Default, Serialize)]
struct TriggerResponse {
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
}

async fn trigger(
    State(triggers): State<Arc<HashMap<String, String>>>,
    Path(name): Path<String>,
) -> (StatusCode, Json<TriggerResponse>) {
    let Some(cmd) = triggers.get(&name) else {
        return (StatusCode::BAD_REQUEST, Json(TriggerResponse::default()));
    };

    info!(trigger = %name, command = %cmd, "Executing trigger");

    let Ok(output) = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::null())
        .output()
        .await
    else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(TriggerResponse::default()),
        );
    };

    (
        StatusCode::OK,
        Json(TriggerResponse {
            exit_code: output.status.code(),
            stdout: Some(String::from_utf8_lossy(&output.stdout).into_owned()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).into_owned()),
        }),
    )
}
