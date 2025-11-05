use serde::{Deserialize, Serialize};
use solana_sdk::account::Account;
use uuid::Uuid;

/// Fork identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForkId(pub Uuid);

impl ForkId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for ForkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ForkId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Fork metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkInfo {
    pub fork_id: ForkId,
    pub rpc_url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub account_count: usize,
}

impl ForkInfo {
    pub fn new(fork_id: ForkId, base_url: &str, account_count: usize) -> Self {
        let now = chrono::Utc::now();
        Self {
            fork_id: fork_id.clone(),
            rpc_url: format!("{}/rpc/{}", base_url, fork_id),
            created_at: now,
            expires_at: now + chrono::Duration::minutes(15),
            account_count,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn remaining_minutes(&self) -> i64 {
        let now = chrono::Utc::now();
        let duration = self.expires_at.signed_duration_since(now);
        duration.num_minutes().max(0)
    }
}

/// API request/response types
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateForkRequest {
    pub accounts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateForkResponse {
    pub success: bool,
    #[serde(rename = "forkId")]
    pub fork_id: String,
    #[serde(rename = "rpcUrl")]
    pub rpc_url: String,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "expiresAt")]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "accountCount")]
    pub account_count: usize,
    #[serde(rename = "ttlMinutes")]
    pub ttl_minutes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetForkResponse {
    pub success: bool,
    #[serde(rename = "forkId")]
    pub fork_id: String,
    #[serde(rename = "rpcUrl")]
    pub rpc_url: String,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "expiresAt")]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "remainingMinutes")]
    pub remaining_minutes: i64,
    #[serde(rename = "accountCount")]
    pub account_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteForkResponse {
    pub success: bool,
    pub message: String,
    #[serde(rename = "forkId")]
    pub fork_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub success: bool,
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetails,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccountData {
    pub lamports: u64,
    pub data: String, // base64
    pub owner: String,
    pub executable: bool,
}

impl AccountData {
    pub fn from_account(account: &Account) -> Self {
        use base64::Engine;
        Self {
            lamports: account.lamports,
            data: base64::engine::general_purpose::STANDARD.encode(&account.data),
            owner: account.owner.to_string(),
            executable: account.executable,
        }
    }

    pub fn to_account(&self) -> anyhow::Result<Account> {
        use base64::Engine;
        Ok(Account {
            lamports: self.lamports,
            data: base64::engine::general_purpose::STANDARD.decode(&self.data)?,
            owner: self.owner.parse()?,
            executable: self.executable,
            rent_epoch: 0,
        })
    }
}

/// JSON-RPC types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}
