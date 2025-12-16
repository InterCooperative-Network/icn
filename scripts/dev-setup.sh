#!/bin/bash
# ICN Development Environment Setup
# This script installs all tools needed for ICN development

set -e

echo "🔧 Setting up ICN development environment..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust is not installed. Install from https://rustup.rs/"
    exit 1
fi

echo "✅ Rust found: $(rustc --version)"

# Install Rust development tools
echo "📦 Installing Rust development tools..."

cargo install cargo-watch --locked || echo "⚠️  cargo-watch already installed"
cargo install cargo-audit --locked || echo "⚠️  cargo-audit already installed"
cargo install cargo-tarpaulin --locked || echo "⚠️  cargo-tarpaulin already installed"
cargo install cargo-outdated --locked || echo "⚠️  cargo-outdated already installed"

# Install Node.js tools if Node.js is available
if command -v npm &> /dev/null; then
    echo "📦 Installing Node.js development tools..."
    npm install -g @commitlint/cli @commitlint/config-conventional || echo "⚠️  commitlint already installed"
fi

# Setup pre-commit hooks
echo "🔗 Setting up pre-commit hooks..."

cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
# Pre-commit hook for ICN

echo "Running pre-commit checks..."

# Check formatting
echo "Checking Rust formatting..."
cd icn
if ! cargo fmt --all -- --check; then
    echo "❌ Formatting check failed. Run 'cargo fmt --all' to fix."
    exit 1
fi

# Run clippy
echo "Running clippy..."
if ! cargo clippy --workspace --all-targets -- -D warnings; then
    echo "❌ Clippy check failed. Fix warnings before committing."
    exit 1
fi

echo "✅ Pre-commit checks passed!"
EOF

chmod +x .git/hooks/pre-commit

echo "✅ Pre-commit hook installed!"

# Setup commit-msg hook for conventional commits
echo "🔗 Setting up commit message validation..."

cat > .git/hooks/commit-msg << 'EOF'
#!/bin/bash
# Commit message validation hook

COMMIT_MSG=$(cat "$1")

# Check conventional commit format
if ! echo "$COMMIT_MSG" | grep -qE '^(feat|fix|docs|style|refactor|test|chore|perf|ci|build|revert)(\(.+\))?: .{1,50}'; then
    echo "❌ Commit message does not follow conventional commits format!"
    echo ""
    echo "Format: <type>(<scope>): <description>"
    echo ""
    echo "Types: feat, fix, docs, style, refactor, test, chore, perf, ci, build, revert"
    echo ""
    echo "Example: feat(gateway): add WebSocket authentication"
    exit 1
fi

echo "✅ Commit message validated!"
EOF

chmod +x .git/hooks/commit-msg

echo "✅ Commit message validation installed!"

# Create .envrc for direnv users (optional)
if command -v direnv &> /dev/null; then
    echo "📝 Creating .envrc for direnv..."
    cat > .envrc << 'EOF'
# ICN development environment
export ICN_LOG=debug
export RUST_BACKTRACE=1
export RUST_LOG=icn=debug
EOF
    echo "✅ .envrc created. Run 'direnv allow' to enable."
fi

echo ""
echo "✅ Development environment setup complete!"
echo ""
echo "📚 Next steps:"
echo "  1. Build the project: cd icn && cargo build"
echo "  2. Run tests: cargo test --workspace"
echo "  3. Start daemon: cargo run --bin icnd"
echo "  4. Use icnctl: cargo run --bin icnctl -- --help"
echo ""
echo "📖 Read CONTRIBUTING.md for development workflow"
