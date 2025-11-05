use crate::{
    fork::ForkManager, CreateForkRequest, CreateForkResponse, DeleteForkResponse, ErrorDetails,
    ErrorResponse, ForkId, GetForkResponse, HealthResponse, JsonRpcRequest,
};
use axum::response::IntoResponse;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::error;

pub type AppState = Arc<ForkManager>;

// Global start time for uptime tracking
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Health check
pub async fn health() -> Json<HealthResponse> {
    let uptime = START_TIME
        .get()
        .map(|t| format_duration(t.elapsed()))
        .unwrap_or_else(|| "unknown".to_string());

    Json(HealthResponse {
        success: true,
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime,
        timestamp: chrono::Utc::now(),
    })
}

/// Create a new fork
pub async fn create_fork(
    State(manager): State<AppState>,
    Json(req): Json<CreateForkRequest>,
) -> Result<(StatusCode, Json<CreateForkResponse>), (StatusCode, Json<ErrorResponse>)> {
    match manager.create_fork(req.accounts).await {
        Ok(fork_info) => {
            let response = CreateForkResponse {
                success: true,
                fork_id: fork_info.fork_id.to_string(),
                rpc_url: fork_info.rpc_url.clone(),
                created_at: fork_info.created_at,
                expires_at: fork_info.expires_at,
                account_count: fork_info.account_count,
                ttl_minutes: 15,
            };
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            error!("Failed to create fork: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    success: false,
                    error: ErrorDetails {
                        code: "FORK_CREATION_FAILED".to_string(),
                        message: "Failed to create fork".to_string(),
                        details: Some(e.to_string()),
                    },
                }),
            ))
        }
    }
}

/// Get fork status
pub async fn get_fork(
    State(manager): State<AppState>,
    Path(fork_id): Path<String>,
) -> Result<Json<GetForkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let fork_id: ForkId = fork_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                success: false,
                error: ErrorDetails {
                    code: "INVALID_FORK_ID".to_string(),
                    message: "Invalid fork ID format".to_string(),
                    details: None,
                },
            }),
        )
    })?;

    match manager.touch_fork(&fork_id).await {
        Ok(Some(info)) => Ok(Json(GetForkResponse {
            success: true,
            fork_id: info.fork_id.to_string(),
            rpc_url: info.rpc_url.clone(),
            status: if info.is_expired() {
                "expired".to_string()
            } else {
                "active".to_string()
            },
            created_at: info.created_at,
            expires_at: info.expires_at,
            remaining_minutes: info.remaining_minutes(),
            account_count: info.account_count,
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                success: false,
                error: ErrorDetails {
                    code: "FORK_NOT_FOUND".to_string(),
                    message: "Fork not found or already deleted".to_string(),
                    details: None,
                },
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: ErrorDetails {
                    code: "INTERNAL_ERROR".to_string(),
                    message: "Failed to retrieve fork information".to_string(),
                    details: Some(e.to_string()),
                },
            }),
        )),
    }
}

/// Delete a fork
pub async fn delete_fork(
    State(manager): State<AppState>,
    Path(fork_id): Path<String>,
) -> Result<Json<DeleteForkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let fork_id: ForkId = fork_id.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                success: false,
                error: ErrorDetails {
                    code: "INVALID_FORK_ID".to_string(),
                    message: "Invalid fork ID format".to_string(),
                    details: None,
                },
            }),
        )
    })?;

    match manager.delete_fork(&fork_id).await {
        Ok(_) => Ok(Json(DeleteForkResponse {
            success: true,
            message: "Fork successfully deleted".to_string(),
            fork_id: fork_id.to_string(),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                success: false,
                error: ErrorDetails {
                    code: "DELETE_FAILED".to_string(),
                    message: "Failed to delete fork".to_string(),
                    details: Some(e.to_string()),
                },
            }),
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
