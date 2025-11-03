use crate::{
    AccountData, ForkId, ForkInfo, JsonRpcError, JsonRpcRequest, JsonRpcResponse, Storage,
};
use anyhow::Result;
use base64::Engine;
use litesvm::LiteSVM;
use serde_json::json;
use solana_sdk::{account::Account, pubkey::Pubkey, transaction::Transaction};
use solana_sysvar::clock::Clock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::info;

/// Manages all active forks in-memory
pub struct ForkManager {
    storage: Storage,
    forks: Arc<RwLock<HashMap<ForkId, Arc<Mutex<LiteSVM>>>>>,
    base_url: String,
    solana_rpc: String,
}

impl ForkManager {
    pub fn new(storage: Storage, base_url: String, solana_rpc: String) -> Self {
        Self {
            storage,
            forks: Arc::new(RwLock::new(HashMap::new())),
            base_url,
            solana_rpc,
        }
    }

    /// Create a new fork
    pub async fn create_fork(&self, account_pubkeys: Vec<String>) -> Result<ForkInfo> {
        let fork_id = ForkId::new();
        info!(
            "Creating fork {} with {} accounts",
            fork_id,
            account_pubkeys.len()
        );

        // Fetch accounts from mainnet
        let accounts = self.fetch_mainnet_accounts(&account_pubkeys).await?;

        // Create new liteSVM instance
        let mut svm = LiteSVM::new();
        for (pubkey, account) in accounts {
            svm.set_account(pubkey, account)?;
        }

        // Initialize chain context (slot, blockhash best-effort)
        self.initialize_chain_context(&mut svm).await.ok();

        // Store in memory
        let mut forks = self.forks.write().await;
        forks.insert(fork_id.clone(), Arc::new(Mutex::new(svm)));

        // Save metadata to Redis
        let fork_info = ForkInfo::new(fork_id, &self.base_url);
        self.storage.save_fork(&fork_info).await?;

        info!("Fork {} created successfully", fork_info.fork_id);
        Ok(fork_info)
    }

    /// Refresh fork TTL and return updated info
    pub async fn touch_fork(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        self.storage.refresh_fork(fork_id).await
    }

    /// Get fork info
    pub async fn get_fork_info(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        self.storage.get_fork(fork_id).await
    }

    /// Increment slot by 1
    pub fn increment_slot(svm: &mut LiteSVM) {
        let mut clock = svm.get_sysvar::<Clock>();
        clock.slot += 1;
        svm.set_sysvar::<Clock>(&clock);
    }

    /// Delete a fork
    pub async fn delete_fork(&self, fork_id: &ForkId) -> Result<()> {
        let mut forks = self.forks.write().await;
        forks.remove(fork_id);
        self.storage.delete_fork(fork_id).await?;
        info!("Fork {} deleted", fork_id);
        Ok(())
    }

    /// Handle RPC request for a specific fork
    pub async fn handle_rpc(&self, fork_id: &ForkId, req: JsonRpcRequest) -> JsonRpcResponse {
        // Refresh TTL on any interaction
        let _ = self.storage.refresh_fork(fork_id).await;

        let forks = self.forks.read().await;
        let svm = match forks.get(fork_id) {
            Some(svm) => svm.clone(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Fork not found or expired".to_string(),
                    }),
                };
            }
        };

        drop(forks); // Release read lock

        let mut svm = svm.lock().await;
        self.process_rpc_method(&mut svm, req).await
    }

    /// Set account data on a fork
    pub async fn set_account(
        &self,
        fork_id: &ForkId,
        pubkey: &Pubkey,
        account: Account,
    ) -> Result<()> {
        let forks = self.forks.read().await;
        let svm = forks
            .get(fork_id)
            .ok_or_else(|| anyhow::anyhow!("Fork not found"))?;
        let mut svm = svm.lock().await;
        svm.set_account(*pubkey, account)?;
        Ok(())
    }

    /// Fetch accounts from mainnet
    async fn fetch_mainnet_accounts(&self, pubkeys: &[String]) -> Result<HashMap<Pubkey, Account>> {
        if pubkeys.is_empty() {
            return Ok(HashMap::new());
        }

        let client = reqwest::Client::new();
        let response = client
            .post(&self.solana_rpc)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getMultipleAccounts",
                "params": [
                    pubkeys,
                    {"encoding": "base64", "commitment": "confirmed"}
                ]
            }))
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        let accounts_data = data["result"]["value"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;

        let mut accounts = HashMap::new();
        for (i, account_data) in accounts_data.iter().enumerate() {
            if account_data.is_null() {
                continue;
            }

            let pubkey: Pubkey = pubkeys[i].parse()?;
            let lamports = account_data["lamports"].as_u64().unwrap_or(0);
            let owner: Pubkey = account_data["owner"].as_str().unwrap_or("").parse()?;
            let executable = account_data["executable"].as_bool().unwrap_or(false);
            let rent_epoch = account_data["rentEpoch"].as_u64().unwrap_or(0);

            let data = if let Some(data_array) = account_data["data"].as_array() {
                if data_array.len() >= 1 {
                    base64::engine::general_purpose::STANDARD
                        .decode(data_array[0].as_str().unwrap_or(""))?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            accounts.insert(
                pubkey,
                Account {
                    lamports,
                    data,
                    owner,
                    executable,
                    rent_epoch,
                },
            );
        }

        Ok(accounts)
    }

    /// Initialize the fork's chain context from the upstream RPC (slot only; blockhash best-effort).
    async fn initialize_chain_context(&self, svm: &mut LiteSVM) -> Result<()> {
        // Fetch latest blockhash (for context.slot) and getSlot explicitly as fallback
        let client = reqwest::Client::new();
        let lb_resp = client
            .post(&self.solana_rpc)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getLatestBlockhash",
                "params": [{"commitment": "confirmed"}]
            }))
            .send()
            .await?;
        let lb_json: serde_json::Value = lb_resp.json().await?;
        let mut slot = lb_json["result"]["context"]["slot"].as_u64().unwrap_or(0);
        if slot == 0 {
            let slot_resp = client
                .post(&self.solana_rpc)
                .json(&json!({"jsonrpc":"2.0","id":2,"method":"getSlot"}))
                .send()
                .await?;
            let slot_json: serde_json::Value = slot_resp.json().await?;
            slot = slot_json["result"].as_u64().unwrap_or(0);
        }

        // Set Clock sysvar slot to match upstream
        if slot > 0 {
            let mut clock: Clock = svm.get_sysvar::<Clock>();
            clock.slot = slot;
            svm.set_sysvar::<Clock>(&clock);
        }
        Ok(())
    }

    /// Process RPC methods
    async fn process_rpc_method(&self, svm: &mut LiteSVM, req: JsonRpcRequest) -> JsonRpcResponse {
        let clock: Clock = svm.get_sysvar::<Clock>();
        let current_slot = clock.slot;

        let result = match req.method.as_str() {
            "getBalance" => self.rpc_get_balance(svm, &req.params),
            "getAccountInfo" => self.rpc_get_account_info(svm, &req.params),
            "sendTransaction" => self.rpc_send_transaction(svm, &req.params),
            "setAccount" => self.rpc_set_account(svm, &req.params).await,
            "getLatestBlockhash" => Ok(json!({
                "context": {"slot": current_slot},
                "value": {
                    "blockhash": svm.latest_blockhash().to_string(),
                    "lastValidBlockHeight": current_slot
                }
            })),
            _ => Err(anyhow::anyhow!("Method not supported")),
        };

        match result {
            Ok(res) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: Some(res),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: e.to_string(),
                }),
            },
        }
    }

    fn rpc_get_balance(
        &self,
        svm: &LiteSVM,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let pubkey: Pubkey = params
            .as_ref()
            .and_then(|p| p[0].as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?
            .parse()?;

        let balance = svm.get_account(&pubkey).map(|a| a.lamports).unwrap_or(0);
        let clock: Clock = svm.get_sysvar::<Clock>();
        let current_slot = clock.slot;
        Ok(json!({"context": {"slot": current_slot}, "value": balance}))
    }

    fn rpc_get_account_info(
        &self,
        svm: &LiteSVM,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let pubkey: Pubkey = params
            .as_ref()
            .and_then(|p| p[0].as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing pubkey"))?
            .parse()?;

        let clock: Clock = svm.get_sysvar::<Clock>();
        let current_slot = clock.slot;

        match svm.get_account(&pubkey) {
            Some(account) => {
                let data = AccountData::from_account(&account);
                Ok(json!({
                    "context": {"slot": current_slot},
                    "value": {
                        "lamports": data.lamports,
                        "owner": data.owner,
                        "data": [data.data, "base64"],
                        "executable": data.executable,
                        "rentEpoch": 0 // TODO: implement rent epoch
                    }
                }))
            }
            None => Ok(json!({"context": {"slot": current_slot}, "value": null})),
        }
    }

    fn rpc_send_transaction(
        &self,
        svm: &mut LiteSVM,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let tx_data = params
            .as_ref()
            .and_then(|p| p[0].as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing transaction"))?;

        let tx_bytes = base64::engine::general_purpose::STANDARD.decode(tx_data)?;
        let transaction: Transaction = bincode::deserialize(&tx_bytes)?;

        let result = svm
            .send_transaction(transaction)
            .map_err(|e| anyhow::anyhow!("Failed to send transaction: {:#?}", e))?;
        
        // Increment slot after transaction
        Self::increment_slot(svm);
        
        Ok(json!(result.signature.to_string()))
    }

    async fn rpc_set_account(
        &self,
        svm: &mut LiteSVM,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let pubkey: Pubkey = params
            .as_ref()
            .and_then(|p| p[0].as_str())
            .ok_or_else(|| anyhow::anyhow!("Failed to parse the public key"))?
            .parse()?;

        let accounts = self.fetch_mainnet_accounts(&[pubkey.to_string()]).await?;
        let clock: Clock = svm.get_sysvar::<Clock>();
        let current_slot = clock.slot;

        let account = accounts[&pubkey].clone();
        svm.set_account(pubkey, account)?;

        Ok(json!({"context": {"slot": current_slot}, "value": null}))
    }
}

// curl -X POST http://localhost:8080/rpc/c6193d87-8e44-4a09-bb61-848dc54dc1dc \
//   -H "Content-Type: application/json" \
//   -d '{
//     "jsonrpc": "2.0",
//     "id": 1,
//     "method": "getBalance",
//     "params": ["9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"]
//   }'
