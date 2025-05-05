.PHONY: all build test clean fmt clippy doc

all: build test

build:
	cargo build

build-release:
	cargo build --release

test:
	cargo test --workspace

clean:
	cargo clean

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace -- -D warnings

doc:
	cargo doc --no-deps --workspace

run:
	cargo run

install:
	cargo install --path cli

check:
	cargo check --workspace

fix:
	cargo fix --allow-dirty --workspace

pre-commit: fmt clippy test
	@echo "Pre-commit checks passed!"

# Helper for creating a new crate
new-crate:
	@read -p "Enter crate name: " name; \
	mkdir -p crates/$$name/src; \
	echo 'pub fn add(left: usize, right: usize) -> usize { left + right }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn it_works() {\n        assert_eq!(2 + 2, 4);\n    }\n}' > crates/$$name/src/lib.rs; \
	echo '[package]\nname = "icn-'$$name'"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n' > crates/$$name/Cargo.toml; \
	echo "Created crate: crates/$$name"

# Helper for setting up pre-commit hook
setup-hooks:
	@mkdir -p .git/hooks
	@echo '#!/bin/sh\nmake pre-commit' > .git/hooks/pre-commit
	@chmod +x .git/hooks/pre-commit
	@echo "Pre-commit hook installed"

.DEFAULT_GOAL := all 