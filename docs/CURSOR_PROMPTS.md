# ICN Development Cursor Prompts

This document provides a collection of useful prompts for Cursor AI to help with common ICN development tasks. These prompts are designed to make development more efficient by leveraging AI assistance.

## Codebase Navigation and Understanding

### Explain Core Components

```
Explain the core components of the ICN project and how they interrelate. Focus on the runtime system, wallet, and AgoraNet components. Include a diagram showing the relationships.
```

### Trace Execution Flow

```
Trace the execution flow for a proposal from creation through voting, execution, receipt generation, and verification. Show the key functions and modules involved at each step.
```

### Analyze Dependencies

```
Analyze the dependencies between the icn-wallet-* crates and identify any potential circular dependencies or areas that could be decoupled for better modularity.
```

## Development Tasks

### Fix Crate Dependencies

```
Fix the dependencies in the [crate_name] crate. It needs the following dependencies: [list_dependencies]. Configure appropriate feature flags if needed.
```

Example:
```
Fix the dependencies in `icn-wallet-actions` to support serde serialization, error handling with thiserror, and async operations with tokio.
```

### Add a New Module

```
Create a new module `[module_name]` in crate `[crate_name]` with the following functionality: [description]. Ensure it follows project code conventions and is properly integrated.
```

Example:
```
Create a new module `receipt_manager` in crate `icn-wallet-sync` that handles the storage, retrieval, and verification of execution receipts. It should use the existing storage layer and integrate with the identity module for verification.
```

### Write Integration Tests

```
Write an integration test that demonstrates the flow from [start_point] to [end_point]. Include appropriate mocks and assertions.
```

Example:
```
Write an integration test that demonstrates the flow from CCL policy compilation to WASM execution to DAG anchoring to ExecutionReceipt verification. Mock any external dependencies.
```

### Debug an Issue

```
Help debug this error: [error_message]. Here's the code that's causing it: [code_snippet]. Suggest fixes that maintain compatibility with the existing API.
```

## Code Quality and Maintenance

### Refactor for Clarity

```
Refactor this function to improve readability and maintainability while preserving its behavior: [function_code]
```

### Add Documentation

```
Add comprehensive documentation to this module/struct/function, including examples: [code_to_document]
```

### Format and Lint

```
Check the code in [file_or_module] for lint issues and suggest fixes. Apply rustfmt formatting and ensure it meets the project's code style guidelines.
```

### Optimize Performance

```
Analyze this function for performance bottlenecks and suggest optimizations: [function_code]
```

## Federation Development

### Generate Genesis Script

```
Create a script that initializes a federation identity, generates the genesis bundle, signs it into a trust bundle, and anchors it to the DAG. Include proper error handling and security measures.
```

### CCL Policy Development

```
Help me write a CCL policy for [governance_scenario]. The policy should handle [specific_conditions] and authorize [specific_actions].
```

Example:
```
Help me write a CCL policy for federation fund allocation. The policy should handle proposal amounts, require different approval thresholds based on amount tiers, and authorize the "federation:allocate_funds" action.
```

### Security Audit

```
Perform a security audit on this code: [code_snippet]. Identify potential vulnerabilities related to encryption, signature verification, or access control.
```

## Operational Tasks

### CI/CD Pipeline Setup

```
Help me set up a GitHub Actions workflow for the ICN project that runs cargo check, cargo test, cargo clippy, and optionally cargo deny.
```

### Deployment Script

```
Create a deployment script for setting up an ICN federation node with the following requirements: [requirements_list]
```

### Monitoring Configuration

```
Help me set up a Prometheus/Grafana configuration to monitor the following metrics from my ICN federation node: [metrics_list]
```

## User Documentation

### User Guide Section

```
Write a user guide section explaining how to [task_description] with the ICN wallet. Include command examples and expected outputs.
```

Example:
```
Write a user guide section explaining how to import a federation trust bundle, request membership, and participate in governance with the ICN wallet. Include command examples and expected outputs.
```

### API Documentation

```
Generate OpenAPI documentation for the [api_endpoint] endpoints in the AgoraNet service.
```

### Troubleshooting Guide

```
Create a troubleshooting guide for common issues with [component], including symptoms, causes, and solutions.
```

## How to Use These Prompts

1. Copy the prompt that best matches your task
2. Paste it into Cursor AI's input
3. Replace the placeholders (`[placeholder]`) with your specific details
4. Adjust the prompt as needed for your specific context
5. Execute the prompt and interact with the AI to refine the results

Remember that these prompts are starting points - you can and should modify them based on your specific needs and context. 