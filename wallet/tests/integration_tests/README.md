# ICN Wallet Integration Tests

This directory contains the integration test suite for the ICN Wallet ecosystem, which includes:

- **Wallet API/CLI**: The primary driver for user interactions
- **AgoraNet**: The discussion and deliberation platform
- **Runtime**: The execution environment for proposals

## Overview

The integration tests are designed to:

1. Start all services using Docker Compose
2. Verify services are healthy and running
3. Execute realistic user workflows via the Wallet CLI/API
4. Verify the correct interactions and state changes across all components
5. Tear down the environment

## Test Structure

- `test_orchestrator.py`: Main Python test orchestrator
- `test_cli_json.sh`: Bash script to test CLI JSON output options
- `run_tests.sh`: Script to run the integration tests with proper environment setup
- `requirements.txt`: Python dependencies

## Test Scenarios

The following scenarios are implemented:

1. **Proposal Lifecycle**: Tests the complete lifecycle of a proposal from creation to execution
   - Create and sign a proposal
   - Find the related thread in AgoraNet
   - Link a credential to the thread
   - Vote on the proposal
   - Create execution receipt
   - Notify about execution
   - Sync trust bundles

2. **State Synchronization**: Tests synchronization of state between components
   - Sync trust bundles
   - List trust bundles
   - Check guardian status
   - Register as a guardian
   - Verify synchronization across components

## Setup Requirements

- Docker and Docker Compose
- Python 3.6+
- Rust toolchain

## Running the Tests

From the project root, run:

```bash
# Run all tests
./tests/run_integration_tests.sh

# Run with specific options
./tests/run_integration_tests.sh --skip-docker  # Skip Docker setup if already running
```

Or from the tests directory:

```bash
# Run just the Python tests
cd tests/integration_tests
./run_tests.sh

# Run CLI tests only
./test_cli_json.sh
```

## Adding New Tests

To add a new test scenario:

1. Create a new class that inherits from `TestScenario` in `test_orchestrator.py`
2. Implement the required methods: `setup()`, `execute()`, `verify()`, `cleanup()`
3. Add your new scenario to the `scenarios` list in the `run_scenarios()` function
4. Run the tests to verify your new scenario

## Docker Environment

The Docker Compose environment includes:

- **runtime**: Mock ICN Runtime service
- **agoranet**: Mock AgoraNet service
- **wallet-api**: The Wallet API service

The services are configured to wait for dependencies and have proper health checks. 