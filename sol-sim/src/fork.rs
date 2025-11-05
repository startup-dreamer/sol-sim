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
use tracing::{debug, error, info, warn};

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
        info!(
            "Setting {} accounts in order (program data before programs)",
            accounts.len()
        );
        for (pubkey, account) in accounts {
            svm.set_account(pubkey, account)?;
        }

        // Initialize chain context (slot, blockhash best-effort)
        self.initialize_chain_context(&mut svm).await.ok();

        // Store in memory
        let mut forks = self.forks.write().await;
        forks.insert(fork_id.clone(), Arc::new(Mutex::new(svm)));

        // Save metadata to in-memory storage
        let account_count = account_pubkeys.len();
        let fork_info = ForkInfo::new(fork_id, &self.base_url, account_count);
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
    /// Fetch accounts from mainnet recursively, getting all accounts in reverse order of ownership
    /// Returns Vec to preserve insertion order (program data before programs)
    async fn fetch_mainnet_accounts(&self, pubkeys: &[String]) -> Result<Vec<(Pubkey, Account)>> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_accounts = Vec::new();
        let mut processed_pubkeys = std::collections::HashSet::new();

        self.fetch_accounts_recursive(pubkeys, &mut all_accounts, &mut processed_pubkeys)
            .await?;

        // Sort accounts to ensure correct order for liteSVM:
        // 1. Non-executable accounts first
        // 2. BPF program data accounts (non-executable, owned by BPF loader)
        // 3. BPF programs (executable, owned by BPF loader)
        // 4. Other executable accounts
        let bpf_loader = "BPFLoaderUpgradeab1e11111111111111111111111"
            .parse::<Pubkey>()
            .unwrap();

        all_accounts.sort_by_key(|(_, account)| {
            let is_bpf_owner = account.owner == bpf_loader;
            match (account.executable, is_bpf_owner) {
                (false, false) => 0, // Non-executable, non-BPF
                (false, true) => 1,  // Program data accounts (must come before programs)
                (true, true) => 2,   // BPF programs (need program data to be set first)
                (true, false) => 3,  // Other executable accounts
            }
        });

        Ok(all_accounts)
    }

    /// Recursive helper function to fetch accounts and their dependencies
    async fn fetch_accounts_recursive(
        &self,
        pubkeys: &[String],
        all_accounts: &mut Vec<(Pubkey, Account)>,
        processed_pubkeys: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        // Filter out already processed pubkeys
        let new_pubkeys: Vec<String> = pubkeys
            .iter()
            .filter(|pk| !processed_pubkeys.contains(*pk))
            .cloned()
            .collect();

        if new_pubkeys.is_empty() {
            return Ok(());
        }

        let client = reqwest::Client::new();
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMultipleAccounts",
            "params": [
                new_pubkeys,
                {"encoding": "base64", "commitment": "confirmed"}
            ]
        });

        let response = client
            .post(&self.solana_rpc)
            .json(&request_body)
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        let accounts_data = data["result"]["value"].as_array().ok_or_else(|| {
            anyhow::anyhow!("Invalid response format: missing result.value array")
        })?;

        info!("Received {} account(s) from API", accounts_data.len());

        let mut non_executable_accounts = Vec::new();
        let mut executable_accounts = Vec::new();
        let mut owner_pubkeys = Vec::new();
        let mut program_data_accounts = Vec::new(); // For BPF Upgradeable programs

        // BPF Upgradeable Loader program ID
        let bpf_upgradeable_loader = "BPFLoaderUpgradeab1e11111111111111111111111"
            .parse::<Pubkey>()
            .unwrap();

        for (i, account_data) in accounts_data.iter().enumerate() {
            if account_data.is_null() {
                warn!(
                    "Account {} at index {} is null in API response",
                    new_pubkeys.get(i).unwrap_or(&"unknown".to_string()),
                    i
                );
                processed_pubkeys.insert(new_pubkeys[i].clone());
                continue;
            }

            let pubkey: Pubkey = new_pubkeys[i].parse()?;
            let lamports = account_data["lamports"].as_u64().unwrap_or(0);
            let owner_str = account_data["owner"].as_str().unwrap_or("");
            let owner: Pubkey = owner_str
                .parse()
                .map_err(|e| anyhow::anyhow!("Failed to parse owner '{}': {}", owner_str, e))?;
            let executable = account_data["executable"].as_bool().unwrap_or(false);
            let rent_epoch = account_data["rentEpoch"].as_u64().unwrap_or(0);

            let data = if let Some(data_array) = account_data["data"].as_array() {
                if data_array.len() >= 1 {
                    let data_str = data_array[0].as_str().unwrap_or("");
                    info!(
                        "Account {} data string from API (base64): '{}'",
                        pubkey, data_str
                    );
                    if data_str.is_empty() {
                        warn!("Account {} data string is EMPTY!", pubkey);
                        Vec::new()
                    } else {
                        match base64::engine::general_purpose::STANDARD.decode(data_str) {
                            Ok(decoded) => {
                                info!(
                                    "Account {} decoded data length: {} bytes",
                                    pubkey,
                                    decoded.len()
                                );
                                decoded
                            }
                            Err(e) => {
                                error!(
                                    "Failed to decode base64 data for account {}: {}",
                                    pubkey, e
                                );
                                Vec::new()
                            }
                        }
                    }
                } else {
                    warn!("Account {} data array is empty!", pubkey);
                    Vec::new()
                }
            } else {
                warn!(
                    "Account {} data field is not an array: {:?}",
                    pubkey, account_data["data"]
                );
                Vec::new()
            };

            info!(
                "Final account {}: lamports={}, data_len={}, owner={}, executable={}, rent_epoch={}",
                pubkey, lamports, data.len(), owner, executable, rent_epoch
            );

            // Check if this is a BPF Upgradeable program and extract program data account
            if executable && owner == bpf_upgradeable_loader && data.len() >= 36 {
                // BPF Upgradeable Program account structure:
                // - Bytes 0-3: Account discriminator (3 for Program account)
                // - Bytes 4-35: ProgramData account pubkey (32 bytes)
                let program_data_bytes: [u8; 32] = data[4..36].try_into().unwrap();
                let program_data_pubkey = Pubkey::new_from_array(program_data_bytes);
                let program_data_str = program_data_pubkey.to_string();

                if !processed_pubkeys.contains(&program_data_str) {
                    info!(
                        "Found BPF Upgradeable program {}, adding program data account {}",
                        pubkey, program_data_pubkey
                    );
                    program_data_accounts.push(program_data_str);
                }
            }

            let account = Account {
                lamports,
                data,
                owner,
                executable,
                rent_epoch,
            };

            // Separate accounts by executable status for reverse order processing
            if executable {
                executable_accounts.push((pubkey, account));
            } else {
                non_executable_accounts.push((pubkey, account));
            }

            // Collect owner pubkeys for recursive fetching
            if owner.to_string() != "11111111111111111111111111111111"
                && !processed_pubkeys.contains(&owner.to_string())
            {
                owner_pubkeys.push(owner.to_string());
            }

            processed_pubkeys.insert(new_pubkeys[i].clone());
        }

        // Process in reverse order: non-executable accounts first (in reverse), then executable accounts (in reverse)
        for (pubkey, account) in non_executable_accounts.into_iter().rev() {
            all_accounts.push((pubkey, account));
        }

        for (pubkey, account) in executable_accounts.into_iter().rev() {
            all_accounts.push((pubkey, account));
        }

        // Recursively fetch owner accounts
        if !owner_pubkeys.is_empty() {
            Box::pin(self.fetch_accounts_recursive(
                &owner_pubkeys,
                all_accounts,
                processed_pubkeys,
            ))
            .await?;
        }

        // Recursively fetch program data accounts for BPF Upgradeable programs
        if !program_data_accounts.is_empty() {
            info!(
                "Fetching {} program data account(s) for BPF Upgradeable programs",
                program_data_accounts.len()
            );
            Box::pin(self.fetch_accounts_recursive(
                &program_data_accounts,
                all_accounts,
                processed_pubkeys,
            ))
            .await?;
        }

        Ok(())
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
                if !account.data.is_empty() {
                    debug!(
                        "Account {} data (base64): {}",
                        pubkey,
                        base64::engine::general_purpose::STANDARD.encode(&account.data)
                    );
                } else {
                    warn!("Account {} retrieved from SVM has EMPTY data!", pubkey);
                }

                let data = AccountData::from_account(&account);

                let response = json!({
                    "context": {"slot": current_slot},
                    "value": {
                        "lamports": data.lamports,
                        "owner": data.owner,
                        "data": [data.data, "base64"],
                        "executable": data.executable,
                        "rentEpoch": account.rent_epoch
                    }
                });

                info!(
                    "Returning account info response for {}: {}",
                    pubkey,
                    serde_json::to_string(&response)?
                );
                Ok(response)
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
        let params_array = params
            .as_ref()
            .and_then(|p| p.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid params: expected array"))?;

        // Check if we have account data (2 params) or just pubkey (1 param - fetch from mainnet)
        if params_array.len() == 2 {
            // Custom account data provided
            let pubkey: Pubkey = params_array[0]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid pubkey"))?
                .parse()?;

            let account_data: AccountData = serde_json::from_value(params_array[1].clone())
                .map_err(|e| anyhow::anyhow!("Failed to parse account data: {}", e))?;

            let account = account_data.to_account()?;
            svm.set_account(pubkey, account)?;

            let clock: Clock = svm.get_sysvar::<Clock>();
            Ok(json!({"context": {"slot": clock.slot}, "value": null}))
        } else if params_array.len() == 1 {
            // Fetch from mainnet
            let pubkey: Pubkey = params_array[0]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid pubkey"))?
                .parse()?;

            let accounts = self.fetch_mainnet_accounts(&[pubkey.to_string()]).await?;
            let clock: Clock = svm.get_sysvar::<Clock>();

            // Set all fetched accounts (includes dependencies)
            for (pk, account) in accounts {
                svm.set_account(pk, account)?;
            }

            Ok(json!({"context": {"slot": clock.slot}, "value": null}))
        } else {
            Err(anyhow::anyhow!(
                "Invalid params: expected 1 param (pubkey) or 2 params (pubkey, accountData)"
            ))
        }
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
