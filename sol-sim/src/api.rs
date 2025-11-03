use crate::{fork::ForkManager, CreateForkRequest, CreateForkResponse, ForkId, JsonRpcRequest};
use axum::response::IntoResponse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::error;

pub type AppState = Arc<ForkManager>;

/// Health check
pub async fn health() -> Json<Value> {
    Json(json!({"status": "healthy"}))
}

/// Create a new fork
pub async fn create_fork(
    State(manager): State<AppState>,
    Json(req): Json<CreateForkRequest>,
) -> Result<(StatusCode, Json<CreateForkResponse>), (StatusCode, Json<Value>)> {
    match manager.create_fork(req.accounts).await {
        Ok(fork_info) => {
            let response = CreateForkResponse {
                fork_id: fork_info.fork_id.to_string(),
                rpc_url: fork_info.rpc_url,
                expires_at: fork_info.expires_at,
            };
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            error!("Failed to create fork: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            ))
        }
    }
}

/// Get fork status
pub async fn get_fork(
    State(manager): State<AppState>,
    Path(fork_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let fork_id: ForkId = fork_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid fork ID"})),
        )
    })?;

    match manager.touch_fork(&fork_id).await {
        Ok(Some(info)) => Ok(Json(json!({
            "forkId": info.fork_id.to_string(),
            "rpcUrl": info.rpc_url,
            "status": if info.is_expired() { "expired" } else { "active" },
            "expiresAt": info.expires_at
        }))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Fork not found"})),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

/// Delete a fork
pub async fn delete_fork(
    State(manager): State<AppState>,
    Path(fork_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let fork_id: ForkId = fork_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid fork ID"})),
        )
    })?;

    match manager.delete_fork(&fork_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )),
    }
}

/// Handle RPC requests
pub async fn handle_rpc(
    State(manager): State<AppState>,
    Path(fork_id): Path<String>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let fork_id: ForkId = match fork_id.parse() {
        Ok(id) => id,
        Err(_) => {
            return Json(crate::JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: None,
                error: Some(crate::JsonRpcError {
                    code: -32602,
                    message: "Invalid fork ID".to_string(),
                }),
            });
        }
    };

    Json(manager.handle_rpc(&fork_id, req).await)
}
