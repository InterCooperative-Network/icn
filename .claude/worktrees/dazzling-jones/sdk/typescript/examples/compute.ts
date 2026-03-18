/**
 * Distributed Compute Example
 *
 * Shows how to submit and monitor compute tasks using the ICN Gateway API.
 *
 * Run: npx ts-node examples/compute.ts
 */

import { ICNClient, SignatureProvider } from '../src';
import * as fs from 'fs';
import * as path from 'path';

// Mock signer for example purposes
async function createMockSigner(): Promise<SignatureProvider> {
  return {
    async sign(challenge: string): Promise<string> {
      console.log('Signing challenge...');
      return 'mock-signature-hex';
    },
  };
}

async function main() {
  // Create client
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
  });

  // Authenticate
  const myDid = 'did:icn:example123';
  const signer = await createMockSigner();
  await client.authenticate(myDid, signer, 'my-coop', [
    'compute:read',
    'compute:write',
  ]);
  console.log('Authenticated!\n');

  // ===========================================================================
  // Example 1: Submit CCL Contract Task
  // ===========================================================================
  console.log('--- CCL Task Submission ---');

  const cclContract = {
    name: 'calculator',
    rules: [
      {
        name: 'add',
        params: ['a', 'b'],
        body: [
          {
            Return: {
              Add: [{ Var: 'a' }, { Var: 'b' }],
            },
          },
        ],
      },
    ],
  };

  const cclTask = await client.submitTask({
    code: JSON.stringify(cclContract),
    fuel_limit: 10000,
    priority: 'normal',
    payment_rate: 100, // 100 credits per 1000 fuel
    inputs: { a: 5, b: 3 },
  });

  console.log('CCL Task submitted:');
  console.log('  Task ID:', cclTask.task_id);
  console.log('  Task Hash:', cclTask.task_hash);

  // ===========================================================================
  // Example 2: Check Task Status
  // ===========================================================================
  console.log('\n--- Task Status ---');

  const status = await client.getTaskStatus(cclTask.task_hash);
  console.log('Status:', status.status);
  if (status.executor) {
    console.log('Executor:', status.executor);
  }

  // ===========================================================================
  // Example 3: Wait for Task Completion
  // ===========================================================================
  console.log('\n--- Waiting for Completion ---');

  try {
    const result = await client.waitForTask(cclTask.task_hash, 1000, 30000);
    console.log('Task completed!');
    console.log('  Status:', result.status);
    if (result.result) {
      console.log('  Outcome:', result.result.outcome);
      console.log('  Output:', result.result.output);
      console.log('  Fuel Used:', result.result.fuel_used);
      console.log('  Duration:', result.result.duration_ms, 'ms');
    }
  } catch (error) {
    console.log('Task did not complete in time:', (error as Error).message);
  }

  // ===========================================================================
  // Example 4: Submit WASM Task (if wasm file available)
  // ===========================================================================
  console.log('\n--- WASM Task Submission ---');

  // Check if we have a WASM module to test with
  const wasmPath = path.join(
    __dirname,
    '../../../examples/wasm-compute/target/wasm32-unknown-unknown/release/icn_wasm_example.wasm'
  );

  if (fs.existsSync(wasmPath)) {
    const wasmBytes = fs.readFileSync(wasmPath);
    console.log(`Loaded WASM module: ${wasmBytes.length} bytes`);

    const wasmTask = await client.submitWasmTask(wasmBytes, {
      fuel_limit: 50000,
      priority: 'high',
    });

    console.log('WASM Task submitted:');
    console.log('  Task Hash:', wasmTask.task_hash);

    // Wait for WASM task
    try {
      const wasmResult = await client.waitForTask(wasmTask.task_hash, 1000, 60000);
      console.log('WASM Task completed!');
      console.log('  Output:', wasmResult.result?.output);
    } catch (error) {
      console.log('WASM task timeout:', (error as Error).message);
    }
  } else {
    console.log('WASM module not found. Build it with:');
    console.log('  cd examples/wasm-compute');
    console.log('  cargo build --release --target wasm32-unknown-unknown');
  }

  // ===========================================================================
  // Example 5: Cancel a Task
  // ===========================================================================
  console.log('\n--- Task Cancellation ---');

  // Submit a task we'll cancel
  const taskToCancel = await client.submitTask({
    code: JSON.stringify(cclContract),
    fuel_limit: 100000, // High fuel to ensure it doesn't complete immediately
  });

  console.log('Submitted task to cancel:', taskToCancel.task_hash);

  // Cancel it
  const cancelResult = await client.cancelTask(taskToCancel.task_hash, {
    reason: 'Demonstrating cancellation',
  });

  console.log('Cancellation result:');
  console.log('  Status:', cancelResult.status);
  console.log('  Reason:', cancelResult.reason);

  // ===========================================================================
  // Example 6: Monitor Multiple Tasks
  // ===========================================================================
  console.log('\n--- Batch Task Processing ---');

  // Submit multiple tasks and collect their responses
  const taskHashes: string[] = [];
  for (let i = 0; i < 3; i++) {
    const task = await client.submitTask({
      code: JSON.stringify({
        ...cclContract,
        name: `calculator-${i}`,
      }),
      fuel_limit: 10000,
      inputs: { a: i, b: i * 2 },
    });
    taskHashes.push(task.task_hash);
    console.log(`Task ${i} submitted: ${task.task_hash}`);
  }

  console.log('\nMonitoring task completion...');

  // Wait for all tasks using Promise.all
  try {
    const results = await Promise.all(
      taskHashes.map((hash) => client.waitForTask(hash, 1000, 30000))
    );

    results.forEach((result, i) => {
      console.log(`Task ${i}:`, result.status, result.result?.output);
    });
  } catch (error) {
    console.log('Some tasks did not complete:', (error as Error).message);
  }
}

main().catch(console.error);
