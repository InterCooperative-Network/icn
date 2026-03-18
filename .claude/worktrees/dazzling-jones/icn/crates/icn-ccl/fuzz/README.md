# ICN-CCL Fuzz Testing

This directory contains fuzz tests for the Cooperative Contract Language (CCL) runtime.

## Prerequisites

Fuzz testing requires the nightly Rust toolchain and `cargo-fuzz`:

```bash
rustup install nightly
cargo +nightly install cargo-fuzz
```

## Available Fuzz Targets

### `fuzz_contract_json`
Tests JSON deserialization into `Contract` structs. Exercises serde_json parsing
and CCL type deserialization.

```bash
cargo +nightly fuzz run fuzz_contract_json
```

### `fuzz_contract_validate`
Tests the `Contract::validate()` method on deserialized contracts. Exercises
name validation, depth limits, and structural validation.

```bash
cargo +nightly fuzz run fuzz_contract_validate
```

### `fuzz_interpreter`
Tests the CCL interpreter execution on valid contracts. Exercises expression
evaluation, statement execution, fuel metering, and capability checking.

```bash
cargo +nightly fuzz run fuzz_interpreter
```

## Running Fuzz Tests

### Basic fuzzing (runs until stopped with Ctrl+C):
```bash
cd crates/icn-ccl
cargo +nightly fuzz run <target>
```

### Time-limited fuzzing (recommended for CI):
```bash
cargo +nightly fuzz run <target> -- -max_total_time=300
```

### Run all targets for 5 minutes each:
```bash
for target in fuzz_contract_json fuzz_contract_validate fuzz_interpreter; do
  cargo +nightly fuzz run $target -- -max_total_time=300
done
```

## Corpus

Each fuzz target has its own corpus directory in `corpus/<target>/`. The corpus
is seeded with:

- Valid contract JSON files from `examples/contracts/` and `contracts/`
- Edge case inputs (empty objects, null, minimal contracts)

The corpus grows as the fuzzer discovers new interesting inputs.

## Reproducing Crashes

If a crash is found, a reproduction file is saved in `artifacts/<target>/`.
To reproduce:

```bash
cargo +nightly fuzz run <target> artifacts/<target>/crash-xxx
```

## Adding New Corpus Seeds

Add JSON files to the appropriate corpus directory:
```bash
cp my_contract.json corpus/fuzz_contract_json/
cp my_contract.json corpus/fuzz_contract_validate/
cp my_contract.json corpus/fuzz_interpreter/
```

## Coverage

To generate coverage information:
```bash
cargo +nightly fuzz coverage <target>
```

The coverage report will be available in `coverage/<target>/`.
