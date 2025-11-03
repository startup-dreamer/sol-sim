/**
 * Example Node.js script demonstrating how to use the Solana Fork Simulation Engine
 *
 * Prerequisites:
 * npm install @solana/web3.js axios
 */

const {
  Connection,
  PublicKey,
  Keypair,
  SystemProgram,
  Transaction,
} = require("@solana/web3.js");
const axios = require("axios");

const API_BASE_URL = "http://localhost:8080";
const API_KEY = "test-api-key-12345678901234567890";

// Common Solana addresses
const USDC_MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const SYSTEM_PROGRAM = "11111111111111111111111111111112";

async function createFork() {
  console.log("Creating a new fork...");

  try {
    const response = await axios.post(
      `${API_BASE_URL}/forks`,
      {
        accounts: [USDC_MINT, SYSTEM_PROGRAM],
      },
      {
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${API_KEY}`,
        },
      }
    );

    console.log("Fork created successfully!");
    console.log(`Fork ID: ${response.data.forkId}`);
    console.log(`RPC URL: ${response.data.rpcUrl}`);
    console.log(`Expires at: ${response.data.expiresAt}`);

    return response.data;
  } catch (error) {
    console.error(
      "Failed to create fork:",
      error.response?.data || error.message
    );
    process.exit(1);
  }
}

async function testForkFunctionality(rpcUrl) {
  console.log("\nTesting fork functionality...");

  // Connect to the fork
  const connection = new Connection(rpcUrl, "confirmed");

  // Test 1: Get account info
  console.log("\n1. Getting USDC mint account info...");
  const usdcMintPubkey = new PublicKey(USDC_MINT);
  const accountInfo = await connection.getAccountInfo(usdcMintPubkey);

  if (accountInfo) {
    console.log(`USDC mint account found:`);
    console.log(`  Lamports: ${accountInfo.lamports}`);
    console.log(`  Owner: ${accountInfo.owner.toBase58()}`);
    console.log(`  Data length: ${accountInfo.data.length} bytes`);
    console.log(`  Executable: ${accountInfo.executable}`);
  } else {
    console.log("USDC mint account not found");
  }

  // Test 2: Create and fund a new keypair
  console.log("\n2. Creating and funding a new keypair...");
  const newKeypair = Keypair.generate();
  console.log(`New keypair public key: ${newKeypair.publicKey.toBase58()}`);

  // Airdrop some SOL (this will work because we're on a simulation)
  const airdropSignature = await connection.requestAirdrop(
    newKeypair.publicKey,
    1000000000
  ); // 1 SOL
  await connection.confirmTransaction(airdropSignature);

  const balance = await connection.getBalance(newKeypair.publicKey);
  console.log(`New keypair balance: ${balance / 1e9} SOL`);

  // Test 3: Create a simple transaction
  console.log("\n3. Creating a transfer transaction...");
  const recipientKeypair = Keypair.generate();

  const transaction = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: newKeypair.publicKey,
      toPubkey: recipientKeypair.publicKey,
      lamports: 100000000, // 0.1 SOL
    })
  );

  // Send the transaction
  const signature = await connection.sendTransaction(transaction, [newKeypair]);
  console.log(`Transaction signature: ${signature}`);

  // Confirm the transaction
  await connection.confirmTransaction(signature);

  // Check balances after transfer
  const senderBalance = await connection.getBalance(newKeypair.publicKey);
  const recipientBalance = await connection.getBalance(
    recipientKeypair.publicKey
  );

  console.log(`Sender balance after transfer: ${senderBalance / 1e9} SOL`);
  console.log(
    `Recipient balance after transfer: ${recipientBalance / 1e9} SOL`
  );

  // Test 4: Simulate a transaction
  console.log("\n4. Simulating a transaction...");
  const simulationTransaction = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: newKeypair.publicKey,
      toPubkey: recipientKeypair.publicKey,
      lamports: 50000000, // 0.05 SOL
    })
  );

  const simulation = await connection.simulateTransaction(
    simulationTransaction,
    [newKeypair]
  );
  console.log("Transaction simulation result:");
  console.log(`  Error: ${simulation.value.err}`);
  console.log(`  Logs: ${JSON.stringify(simulation.value.logs, null, 2)}`);
  console.log(`  Units consumed: ${simulation.value.unitsConsumed}`);
}

async function modifyAccountState(forkId, pubkey, newBalance) {
  console.log(`\n5. Modifying account state directly...`);
  console.log(`Setting balance of ${pubkey} to ${newBalance} lamports`);

  try {
    await axios.post(
      `${API_BASE_URL}/forks/${forkId}/accounts`,
      {
        pubkey: pubkey,
        account: {
          lamports: newBalance,
          data: "", // Empty data for system account
          owner: SYSTEM_PROGRAM,
          executable: false,
        },
      },
      {
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${API_KEY}`,
        },
      }
    );

    console.log("Account state updated successfully!");
  } catch (error) {
    console.error(
      "Failed to update account state:",
      error.response?.data || error.message
    );
  }
}

async function deleteFork(forkId) {
  console.log("\n6. Cleaning up - deleting fork...");

  try {
    await axios.delete(`${API_BASE_URL}/forks/${forkId}`, {
      headers: {
        Authorization: `Bearer ${API_KEY}`,
      },
    });

    console.log("Fork deleted successfully!");
  } catch (error) {
    console.error(
      "Failed to delete fork:",
      error.response?.data || error.message
    );
  }
}

async function main() {
  try {
    // Create a fork
    const forkData = await createFork();

    // Test the fork functionality
    await testForkFunctionality(forkData.rpcUrl);

    // Test direct state modification
    const testKeypair = Keypair.generate();
    await modifyAccountState(
      forkData.forkId,
      testKeypair.publicKey.toBase58(),
      5000000000
    );

    // Verify the state change
    const connection = new Connection(forkData.rpcUrl, "confirmed");
    const modifiedBalance = await connection.getBalance(testKeypair.publicKey);
    console.log(`Verified modified balance: ${modifiedBalance / 1e9} SOL`);

    // Clean up
    await deleteFork(forkData.forkId);

    console.log("\nExample completed successfully!");
  } catch (error) {
    console.error("Example failed:", error);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}
