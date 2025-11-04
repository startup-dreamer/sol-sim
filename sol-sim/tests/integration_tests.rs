use anyhow::Result;
use base64::Engine;
use reqwest::Client;
use serde_json::json;
use sol_sim::CreateForkResponse;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use tokio;

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

/// Integration test helper struct
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

    async fn create_fork(&self, accounts: Vec<String>) -> Result<CreateForkResponse> {
        let response = self
            .client
            .post(format!("{}/forks", self.base_url))
            .json(&json!({ "accounts": accounts }))
            .send()
            .await?;

        Ok(response.json().await?)
    }

    async fn get_fork(&self, fork_id: &str) -> Result<serde_json::Value> {
        let response = self
            .client
            .get(format!("{}/forks/{}", self.base_url, fork_id))
            .send()
            .await?;

        Ok(response.json().await?)
    }

    async fn delete_fork(&self, fork_id: &str) -> Result<()> {
        self.client
            .delete(format!("{}/forks/{}", self.base_url, fork_id))
            .send()
            .await?;
        Ok(())
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
}

#[tokio::test]
async fn test_fork_lifecycle() -> Result<()> {
    let ctx = TestContext::new();

    // Create fork
    let fork_response = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    assert!(!fork_response.fork_id.is_empty());
    assert!(fork_response.rpc_url.contains(&fork_response.fork_id));

    // Get fork info
    let fork_info = ctx.get_fork(&fork_response.fork_id).await?;
    assert_eq!(fork_info["forkId"], fork_response.fork_id);

    // Delete fork
    ctx.delete_fork(&fork_response.fork_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_account_operations() -> Result<()> {
    let ctx = TestContext::new();

    // Create fork
    let fork = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    // Generate test keypair
    let test_account = Keypair::new();
    let pubkey = test_account.pubkey().to_string();

    // Set account with custom data
    let set_response = ctx
        .rpc_call(
            &fork.fork_id,
            "setAccount",
            json!([
                pubkey,
                {
                    "lamports": 5_000_000_000u64,
                    "data": "",
                    "owner": "11111111111111111111111111111111",
                    "executable": false
                }
            ]),
        )
        .await?;

    assert!(set_response["result"].is_object());

    // Get balance
    let balance_response = ctx
        .rpc_call(&fork.fork_id, "getBalance", json!([pubkey]))
        .await?;

    assert_eq!(balance_response["result"]["value"], json!(5_000_000_000u64));

    // Get account info
    let account_response = ctx
        .rpc_call(&fork.fork_id, "getAccountInfo", json!([pubkey]))
        .await?;

    assert_eq!(
        account_response["result"]["value"]["lamports"],
        json!(5_000_000_000u64)
    );

    // Cleanup
    ctx.delete_fork(&fork.fork_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_transaction_execution() -> Result<()> {
    let ctx = TestContext::new();

    // Create fork
    let fork = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    // Create test keypairs
    let payer = Keypair::new();
    let recipient = Keypair::new();

    // Fund payer
    ctx.rpc_call(
        &fork.fork_id,
        "setAccount",
        json!([
            payer.pubkey().to_string(),
            {
                "lamports": 10_000_000_000u64,
                "data": "",
                "owner": "11111111111111111111111111111111",
                "executable": false
            }
        ]),
    )
    .await?;

    // Get blockhash
    let blockhash_response = ctx
        .rpc_call(&fork.fork_id, "getLatestBlockhash", json!([]))
        .await?;

    let blockhash_str = blockhash_response["result"]["value"]["blockhash"]
        .as_str()
        .unwrap();
    let blockhash: solana_sdk::hash::Hash = blockhash_str.parse()?;

    // Create transfer transaction
    let transfer_amount = 2_000_000_000u64;
    let instruction = transfer(&payer.pubkey(), &recipient.pubkey(), transfer_amount);

    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], blockhash);

    // Serialize and encode
    let serialized = bincode::serialize(&transaction)?;
    let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized);

    // Send transaction
    let tx_response = ctx
        .rpc_call(&fork.fork_id, "sendTransaction", json!([base64_tx]))
        .await?;

    assert!(tx_response["result"].is_string());
    let signature = tx_response["result"].as_str().unwrap();
    assert!(!signature.is_empty());

    // Verify recipient balance
    let recipient_balance = ctx
        .rpc_call(
            &fork.fork_id,
            "getBalance",
            json!([recipient.pubkey().to_string()]),
        )
        .await?;

    assert_eq!(recipient_balance["result"]["value"], json!(transfer_amount));

    // Cleanup
    ctx.delete_fork(&fork.fork_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_slot_progression() -> Result<()> {
    let ctx = TestContext::new();

    // Create fork
    let fork = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    // Get initial slot
    let blockhash1 = ctx
        .rpc_call(&fork.fork_id, "getLatestBlockhash", json!([]))
        .await?;
    let slot1 = blockhash1["result"]["context"]["slot"].as_u64().unwrap();

    // Fund account and send transaction
    let payer = Keypair::new();
    let recipient = Keypair::new();

    ctx.rpc_call(
        &fork.fork_id,
        "setAccount",
        json!([
            payer.pubkey().to_string(),
            {
                "lamports": 10_000_000_000u64,
                "data": "",
                "owner": "11111111111111111111111111111111",
                "executable": false
            }
        ]),
    )
    .await?;

    let blockhash_str = blockhash1["result"]["value"]["blockhash"].as_str().unwrap();
    let blockhash: solana_sdk::hash::Hash = blockhash_str.parse()?;

    let instruction = transfer(&payer.pubkey(), &recipient.pubkey(), 1_000_000_000);
    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], blockhash);

    let serialized = bincode::serialize(&transaction)?;
    let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized);

    ctx.rpc_call(&fork.fork_id, "sendTransaction", json!([base64_tx]))
        .await?;

    // Get slot after transaction
    let blockhash2 = ctx
        .rpc_call(&fork.fork_id, "getLatestBlockhash", json!([]))
        .await?;
    let slot2 = blockhash2["result"]["context"]["slot"].as_u64().unwrap();

    // Slot should have incremented
    assert!(slot2 > slot1, "Slot should increment after transaction");

    // Cleanup
    ctx.delete_fork(&fork.fork_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_concurrent_transactions() -> Result<()> {
    let ctx = TestContext::new();

    // Create fork
    let fork = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    // Create payer and multiple recipients
    let payer = Keypair::new();
    let recipients: Vec<Keypair> = (0..5).map(|_| Keypair::new()).collect();

    // Fund payer
    ctx.rpc_call(
        &fork.fork_id,
        "setAccount",
        json!([
            payer.pubkey().to_string(),
            {
                "lamports": 50_000_000_000u64,
                "data": "",
                "owner": "11111111111111111111111111111111",
                "executable": false
            }
        ]),
    )
    .await?;

    // Get blockhash
    let blockhash_response = ctx
        .rpc_call(&fork.fork_id, "getLatestBlockhash", json!([]))
        .await?;
    let blockhash_str = blockhash_response["result"]["value"]["blockhash"]
        .as_str()
        .unwrap();
    let blockhash: solana_sdk::hash::Hash = blockhash_str.parse()?;

    // Send multiple transactions
    for recipient in &recipients {
        let instruction = transfer(&payer.pubkey(), &recipient.pubkey(), 1_000_000_000);
        let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
        transaction.sign(&[&payer], blockhash);

        let serialized = bincode::serialize(&transaction)?;
        let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized);

        ctx.rpc_call(&fork.fork_id, "sendTransaction", json!([base64_tx]))
            .await?;
    }

    // Verify all recipients received funds
    for recipient in &recipients {
        let balance = ctx
            .rpc_call(
                &fork.fork_id,
                "getBalance",
                json!([recipient.pubkey().to_string()]),
            )
            .await?;

        assert_eq!(balance["result"]["value"], json!(1_000_000_000u64));
    }

    // Cleanup
    ctx.delete_fork(&fork.fork_id).await?;

    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<()> {
    let ctx = TestContext::new();

    // Try to get non-existent fork
    let result = ctx.get_fork("non-existent-fork-id").await;
    assert!(result.is_ok()); // Should return error response, not panic

    // Create fork
    let fork = ctx
        .create_fork(vec!["11111111111111111111111111111111".to_string()])
        .await?;

    // Try to send transaction with insufficient balance
    let payer = Keypair::new();
    let recipient = Keypair::new();

    let blockhash_response = ctx
        .rpc_call(&fork.fork_id, "getLatestBlockhash", json!([]))
        .await?;
    let blockhash_str = blockhash_response["result"]["value"]["blockhash"]
        .as_str()
        .unwrap();
    let blockhash: solana_sdk::hash::Hash = blockhash_str.parse()?;

    let instruction = transfer(&payer.pubkey(), &recipient.pubkey(), 1_000_000_000);
    let mut transaction = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
    transaction.sign(&[&payer], blockhash);

    let serialized = bincode::serialize(&transaction)?;
    let base64_tx = base64::engine::general_purpose::STANDARD.encode(serialized);

    let tx_response = ctx
        .rpc_call(&fork.fork_id, "sendTransaction", json!([base64_tx]))
        .await?;

    // Should return error
    assert!(tx_response["error"].is_object());

    // Cleanup
    ctx.delete_fork(&fork.fork_id).await?;

    Ok(())
}
