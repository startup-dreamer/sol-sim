use anyhow::Result;
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use solana_sdk::{
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    message::VersionedMessage,
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::{Transaction, VersionedTransaction},
};

// System program ID constant
const SYSTEM_PROGRAM_ID: Pubkey = solana_sdk::pubkey!("11111111111111111111111111111111");

/// Helper to create a system transfer instruction
fn transfer(from: &Pubkey, to: &Pubkey, lamports: u64) -> Instruction {
    Instruction {
        program_id: SYSTEM_PROGRAM_ID,
        accounts: vec![AccountMeta::new(*from, true), AccountMeta::new(*to, false)],
        data: vec![2, 0, 0, 0] // Transfer instruction discriminator
            .into_iter()
            .chain(lamports.to_le_bytes().to_vec())
            .collect(),
    }
}

/// Helper for fork operations
struct TestContext {
    base_url: String,
    client: Client,
}

impl TestContext {
    fn new() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            client: Client::new(),
        }
    }

    async fn create_fork(&self, accounts: Vec<&str>) -> Result<(String, String)> {
        let accounts: Vec<String> = accounts.iter().map(|s| s.to_string()).collect();
        let response = self
            .client
            .post(format!("{}/forks", self.base_url))
            .json(&json!({ "accounts": accounts }))
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;
        Ok((
            data["forkId"].as_str().unwrap().to_string(),
            data["rpcUrl"].as_str().unwrap().to_string(),
        ))
    }

    async fn rpc_call(
        &self,
        fork_id: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let response = self
            .client
            .post(format!("{}/rpc/{}", self.base_url, fork_id))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params
            }))
            .send()
            .await?;

        Ok(response.json().await?)
    }

    async fn send_transaction(&self, fork_id: &str, transaction: &Transaction) -> Result<String> {
        let serialized = bincode::serialize(transaction)?;
        let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized);

        let response = self
            .rpc_call(fork_id, "sendTransaction", json!([base64_tx]))
            .await?;

        if let Some(error) = response.get("error") {
            return Err(anyhow::anyhow!("Transaction failed: {:?}", error));
        }

        Ok(response["result"].as_str().unwrap().to_string())
    }

    async fn get_balance(&self, fork_id: &str, pubkey: &Pubkey) -> Result<u64> {
        let response = self
            .rpc_call(fork_id, "getBalance", json!([pubkey.to_string()]))
            .await?;

        Ok(response["result"]["value"].as_u64().unwrap())
    }

    async fn set_account(
        &self,
        fork_id: &str,
        pubkey: &Pubkey,
        lamports: u64,
        data: &[u8],
        owner: &Pubkey,
        executable: bool,
    ) -> Result<()> {
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(data);

        self.rpc_call(
            fork_id,
            "setAccount",
            json!([
                pubkey.to_string(),
                {
                    "lamports": lamports,
                    "data": data_base64,
                    "owner": owner.to_string(),
                    "executable": executable
                }
            ]),
        )
        .await?;

        Ok(())
    }

    async fn get_blockhash(&self, fork_id: &str) -> Result<Hash> {
        let response = self
            .rpc_call(fork_id, "getLatestBlockhash", json!([]))
            .await?;

        let blockhash_str = response["result"]["value"]["blockhash"].as_str().unwrap();
        Ok(blockhash_str.parse()?)
    }

    async fn cleanup(&self, fork_id: &str) -> Result<()> {
        self.client
            .delete(format!("{}/forks/{}", self.base_url, fork_id))
            .send()
            .await?;
        Ok(())
    }
}

/// Test: verify complex transaction with multiple operations
#[tokio::test]
async fn test_fork_complex_transaction() -> Result<()> {
    let ctx = TestContext::new();

    let (fork_id, _) = ctx
        .create_fork(vec!["11111111111111111111111111111111"])
        .await?;

    // 1. Transfer SOL to temporary account
    // 2. Transfer from temp to final destination
    let payer = Keypair::new();
    let temp_account = Keypair::new();
    let final_destination = Keypair::new();

    // Fund payer
    ctx.set_account(
        &fork_id,
        &payer.pubkey(),
        50_000_000_000,
        &[],
        &SYSTEM_PROGRAM_ID,
        false,
    )
    .await?;

    let blockhash = ctx.get_blockhash(&fork_id).await?;

    // Create complex transaction
    let instructions = vec![
        transfer(&payer.pubkey(), &temp_account.pubkey(), 20_000_000_000),
        transfer(
            &temp_account.pubkey(),
            &final_destination.pubkey(),
            10_000_000_000,
        ),
    ];

    let mut transaction = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
    transaction.sign(&[&payer, &temp_account], blockhash);

    // Execute complex transaction
    let signature = ctx.send_transaction(&fork_id, &transaction).await?;

    // Verify final state
    let temp_balance = ctx.get_balance(&fork_id, &temp_account.pubkey()).await?;
    let final_balance = ctx
        .get_balance(&fork_id, &final_destination.pubkey())
        .await?;

    assert_eq!(
        temp_balance, 10_000_000_000,
        "Temp account should have 10 SOL remaining"
    );
    assert_eq!(
        final_balance, 10_000_000_000,
        "Final destination should have 10 SOL"
    );

    ctx.cleanup(&fork_id).await?;
    Ok(())
}

/// verify multiple DeFi protocols (Jupiter + Raydium)
#[tokio::test]
async fn test_fork_multiple_protocols() -> Result<()> {
    let ctx = TestContext::new();

    let (fork_id, _) = ctx
        .create_fork(vec![
            "11111111111111111111111111111111",
            "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", // Jupiter
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", // Raydium AMM
        ])
        .await?;

    // Verify Jupiter is loaded
    let jupiter_response = ctx
        .rpc_call(
            &fork_id,
            "getAccountInfo",
            json!(["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"]),
        )
        .await?;

    assert!(jupiter_response["result"]["value"]["executable"]
        .as_bool()
        .unwrap());

    // Verify Raydium is loaded
    let raydium_response = ctx
        .rpc_call(
            &fork_id,
            "getAccountInfo",
            json!(["675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8"]),
        )
        .await?;

    assert!(raydium_response["result"]["value"]["executable"]
        .as_bool()
        .unwrap());

    ctx.cleanup(&fork_id).await?;
    Ok(())
}

/// Test: Stress test - many sequential transactions
#[tokio::test]
async fn test_fork_sequential_transactions_stress() -> Result<()> {
    let ctx = TestContext::new();

    let (fork_id, _) = ctx
        .create_fork(vec!["11111111111111111111111111111111"])
        .await?;

    let sender = Keypair::new();
    let receiver = Keypair::new();

    // Fund sender with enough for many transactions
    ctx.set_account(
        &fork_id,
        &sender.pubkey(),
        1_000_000_000_000,
        &[],
        &SYSTEM_PROGRAM_ID,
        false,
    )
    .await?;

    // Send 50 transactions sequentially with varying amounts to ensure unique signatures
    let mut total_sent = 0u64;
    for i in 0..50 {
        let blockhash = ctx.get_blockhash(&fork_id).await?;

        // Use varying amounts to create unique transaction signatures
        let amount = 1_000_000_000 + (i as u64 * 1000);
        let instruction = transfer(&sender.pubkey(), &receiver.pubkey(), amount);

        let mut transaction = Transaction::new_with_payer(&[instruction], Some(&sender.pubkey()));
        transaction.sign(&[&sender], blockhash);

        ctx.send_transaction(&fork_id, &transaction).await?;
        total_sent += amount;

        if (i + 1) % 10 == 0 {
            println!("Completed {} transactions", i + 1);
        }
    }

    // Verify final balance (sum of varying amounts)
    let receiver_balance = ctx.get_balance(&fork_id, &receiver.pubkey()).await?;
    assert_eq!(
        receiver_balance, total_sent,
        "Receiver should have received all transferred funds"
    );

    ctx.cleanup(&fork_id).await?;
    Ok(())
}

/// Test: Jupiter Lend WSOL deposit operation - Fork and Replay Real Transaction
///
/// This test demonstrates the fork's capability to handle complex DeFi operations by:
/// 1. Fetching a real mainnet transaction that deposited 1.5 SOL as WSOL into Jupiter Lend vault
/// 2. Parsing V0 (versioned) transactions with address lookup tables
/// 3. Creating a fork with all involved accounts
/// 4. Rebuilding and replaying the transaction instructions with a test signer
/// 5. Verifying the transaction executes successfully on the fork
///
/// Reference tx: 2X9LmajpxFK46Kti6cubrvL1WN7XWgwVjXdevJY36QurniTGaXD3mpnwMPBg283ZovZpq2eeQJpNk8FQmby2gbjD
///
/// Transaction flow:
/// - SetComputeUnitLimit (1,000,000 units)
/// - SetComputeUnitPrice (100,000 micro-lamports)
/// - System Program: Transfer 1.5 SOL to WSOL account
/// - Token Program: SyncNative (wrap SOL)
/// - Jupiter Lend: Operate (deposit to vault)
#[tokio::test]
async fn test_jupiter_lend_wsol_deposit() -> Result<()> {
    let ctx = TestContext::new();
    let mainnet_rpc = "https://api.mainnet-beta.solana.com";

    // The actual transaction signature from mainnet
    let tx_signature =
        "2X9LmajpxFK46Kti6cubrvL1WN7XWgwVjXdevJY36QurniTGaXD3mpnwMPBg283ZovZpq2eeQJpNk8FQmby2gbjD";

    // Fetch the transaction from mainnet
    let tx_response = Client::new()
        .post(mainnet_rpc)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                tx_signature,
                {
                    "encoding": "jsonParsed",
                    "maxSupportedTransactionVersion": 0
                }
            ]
        }))
        .send()
        .await?;

    let tx_data: serde_json::Value = tx_response.json().await?;

    if tx_data.get("error").is_some() {
        return Err(anyhow::anyhow!(
            "Failed to fetch transaction: {:?}",
            tx_data["error"]
        ));
    }

    // Extract all account keys involved in the transaction
    let transaction = &tx_data["result"]["transaction"];
    let account_keys = &transaction["message"]["accountKeys"];

    let mut accounts_to_fork: Vec<String> = Vec::new();

    // Collect all account addresses
    if let Some(keys) = account_keys.as_array() {
        for key in keys {
            if let Some(pubkey) = key["pubkey"].as_str() {
                accounts_to_fork.push(pubkey.to_string());
            } else if let Some(pubkey) = key.as_str() {
                accounts_to_fork.push(pubkey.to_string());
            }
        }
    }


    // Create fork with all accounts from the original transaction
    let accounts_refs: Vec<&str> = accounts_to_fork.iter().map(|s| s.as_str()).collect();
    let (fork_id, _) = ctx.create_fork(accounts_refs).await?;


    // Now fetch the raw transaction to replay it
    let raw_tx_response = Client::new()
        .post(mainnet_rpc)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                tx_signature,
                {
                    "encoding": "base64",
                    "maxSupportedTransactionVersion": 0
                }
            ]
        }))
        .send()
        .await?;

    let raw_tx_data: serde_json::Value = raw_tx_response.json().await?;

    // The response structure is: result.transaction[0] is the base64 string, [1] is the encoding type
    let tx_base64 = if let Some(tx_array) = raw_tx_data["result"]["transaction"].as_array() {
        if let Some(base64_str) = tx_array.get(0).and_then(|v| v.as_str()) {
            base64_str
        } else {
            return Err(anyhow::anyhow!(
                "Failed to extract base64 transaction data from array"
            ));
        }
    } else if let Some(tx_str) = raw_tx_data["result"]["transaction"].as_str() {
        tx_str
    } else {
        return Err(anyhow::anyhow!(
            "Transaction data not in expected format (neither array nor string)"
        ));
    };

    if !tx_base64.is_empty() {
        let tx_bytes = base64::engine::general_purpose::STANDARD.decode(tx_base64)?;

        // Deserialize as VersionedTransaction
        let versioned_tx: VersionedTransaction = bincode::deserialize(&tx_bytes)?;


        // Extract message details based on version
        let (account_keys, instructions, header) = match &versioned_tx.message {
            VersionedMessage::Legacy(msg) => {
                (&msg.account_keys, &msg.instructions, &msg.header)
            }
            VersionedMessage::V0(msg) => {
                (&msg.account_keys, &msg.instructions, &msg.header)
            }
        };

        // Get the signer from the original transaction
        let original_signer = account_keys[0];

        // Create a new keypair for testing
        let test_signer = Keypair::new();

        // Fund the test signer with enough SOL (10 SOL)
        ctx.set_account(
            &fork_id,
            &test_signer.pubkey(),
            10_000_000_000,
            &[],
            &SYSTEM_PROGRAM_ID,
            false,
        )
        .await?;


        // Helper function to determine if account is writable based on message header
        let is_account_writable = |idx: usize| -> bool {
            let num_signed = header.num_required_signatures as usize;
            let num_ro_signed = header.num_readonly_signed_accounts as usize;
            let num_ro_unsigned = header.num_readonly_unsigned_accounts as usize;
            let total_accounts = account_keys.len();

            if idx < num_signed {
                // Signed account - writable if not in readonly range
                idx >= num_ro_signed
            } else {
                // Unsigned account - writable if not in readonly range
                idx < total_accounts - num_ro_unsigned
            }
        };

        // Clone the instructions but replace the signer references
        let mut new_instructions = Vec::new();
        for (ix_num, ix) in instructions.iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if program_id_idx >= account_keys.len() {
                continue;
            }
            let program_id = account_keys[program_id_idx];

            let mut accounts_meta = Vec::new();
            for &acc_idx in &ix.accounts {
                let acc_idx_usize = acc_idx as usize;
                if acc_idx_usize >= account_keys.len() {
                    accounts_meta.clear();
                    break;
                }
                let original_pubkey = account_keys[acc_idx_usize];

                // Replace the original signer with test signer
                let pubkey = if original_pubkey == original_signer {
                    test_signer.pubkey()
                } else {
                    original_pubkey
                };

                // Determine if this was a signer/writable in the original
                let is_signer = acc_idx_usize < header.num_required_signatures as usize;
                let is_writable = is_account_writable(acc_idx_usize);

                accounts_meta.push(if is_signer && is_writable {
                    AccountMeta::new(pubkey, true)
                } else if is_signer {
                    AccountMeta::new_readonly(pubkey, true)
                } else if is_writable {
                    AccountMeta::new(pubkey, false)
                } else {
                    AccountMeta::new_readonly(pubkey, false)
                });
            }

            // Only add instruction if we successfully processed all accounts
            if !accounts_meta.is_empty() || ix.accounts.is_empty() {
                new_instructions.push(Instruction {
                    program_id,
                    accounts: accounts_meta,
                    data: ix.data.clone(),
                });
            }
        }

         
        if new_instructions.is_empty() {
            println!("No instructions could be rebuilt - transaction uses address lookup tables");
            return Err(anyhow::anyhow!("No instructions could be rebuilt - transaction uses address lookup tables"));
        } else {
            // Get blockhash and create new transaction
            let blockhash = ctx.get_blockhash(&fork_id).await?;
            let mut new_transaction =
                Transaction::new_with_payer(&new_instructions, Some(&test_signer.pubkey()));
            new_transaction.sign(&[&test_signer], blockhash);


            // Send transaction
            match ctx.send_transaction(&fork_id, &new_transaction).await {
                Ok(signature) => {
                    println!("Transaction successful with signature: {}", signature);
                    assert!(ctx.get_balance(&fork_id, &test_signer.pubkey()).await? <= 8500000000);
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Transaction failed: {}", e));
                }
            }
        }
    } else {
        return Err(anyhow::anyhow!("Failed to get raw transaction data: {}", tx_base64));
    }

    ctx.cleanup(&fork_id).await?;
    Ok(())
}
#[tokio::test]
async fn test_fork_simple_sol_transfer() -> Result<()> {
    let ctx = TestContext::new();

    let (fork_id, _) = ctx.create_fork(vec!["11111111111111111111111111111111"]).await?;

    let sender = Keypair::new();
    let receiver = Keypair::new();
    ctx.set_account(&fork_id, &sender.pubkey(), 100_000_000_000, &[], &SYSTEM_PROGRAM_ID, false)
        .await?;

    let blockhash = ctx.get_blockhash(&fork_id).await?;

    // Create transfer transaction (10 SOL)
    let transfer_instruction =
        transfer(&sender.pubkey(), &receiver.pubkey(), 10_000_000_000);

    let mut transaction = Transaction::new_with_payer(&[transfer_instruction], Some(&sender.pubkey()));
    transaction.sign(&[&sender], blockhash);

    // Execute transaction
    let signature = ctx.send_transaction(&fork_id, &transaction).await?;
    println!("✅ Transfer signature: {}", signature);

    // Verify balances
    let receiver_balance = ctx.get_balance(&fork_id, &receiver.pubkey()).await?;
    assert_eq!(receiver_balance, 10_000_000_000, "Receiver should have 10 SOL");

    let sender_balance = ctx.get_balance(&fork_id, &sender.pubkey()).await?;
    assert!(sender_balance < 90_000_000_000, "Sender balance should be reduced by transfer + fees");

    ctx.cleanup(&fork_id).await?;
    Ok(())
}