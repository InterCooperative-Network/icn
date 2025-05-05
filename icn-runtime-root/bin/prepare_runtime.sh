#!/bin/bash

# ICN Runtime Preparation Script
# This script prepares the ICN Runtime for production deployment
# by building, testing, and configuring the runtime

set -e

# Colors for output formatting
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}ICN Runtime Preparation Script${NC}"
echo "========================================"
echo

# Check command-line arguments
DEPLOY_ENV=${1:-"development"}
CONFIG_DIR=${2:-"./config"}

if [ "$DEPLOY_ENV" != "development" ] && [ "$DEPLOY_ENV" != "testnet" ] && [ "$DEPLOY_ENV" != "production" ]; then
    echo -e "${RED}Error: Invalid deployment environment.${NC}"
    echo "Valid environments: development, testnet, production"
    exit 1
fi

echo -e "${YELLOW}Preparing ICN Runtime for ${DEPLOY_ENV} deployment${NC}"
echo "Config directory: ${CONFIG_DIR}"
echo

# Ensure config directory exists
mkdir -p ${CONFIG_DIR}

# Step 1: Build the runtime
echo -e "${YELLOW}Step 1: Building ICN Runtime...${NC}"
cargo build --release
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Build successful${NC}"
else
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

# Step 2: Run tests
echo -e "${YELLOW}Step 2: Running tests...${NC}"
cargo test --release
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Tests passed${NC}"
else
    echo -e "${RED}✗ Tests failed${NC}"
    echo "Review test logs before proceeding"
    read -p "Continue anyway? (y/n) " CONTINUE
    if [ "$CONTINUE" != "y" ]; then
        exit 1
    fi
fi

# Step 3: Run security audit
echo -e "${YELLOW}Step 3: Running security audit...${NC}"
cargo audit
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Security audit passed${NC}"
else
    echo -e "${RED}✗ Security audit found issues${NC}"
    echo "Review security issues before proceeding"
    read -p "Continue anyway? (y/n) " CONTINUE
    if [ "$CONTINUE" != "y" ]; then
        exit 1
    fi
fi

# Step 4: Create default configuration
echo -e "${YELLOW}Step 4: Creating configuration for ${DEPLOY_ENV}...${NC}"
CONFIG_FILE="${CONFIG_DIR}/runtime-config-${DEPLOY_ENV}.toml"

if [ -f "$CONFIG_FILE" ]; then
    echo "Configuration file already exists: ${CONFIG_FILE}"
    read -p "Overwrite? (y/n) " OVERWRITE
    if [ "$OVERWRITE" != "y" ]; then
        echo "Using existing configuration"
    else
        # Generate appropriate configuration based on environment
        case "$DEPLOY_ENV" in
            development)
                cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-dev-1"
mode = "development"
data_dir = "./data"
max_memory_mib = 1024

[identity]
did = "did:icn:node:dev1"
key_file = "./keys/node-key.pem"

[network]
http_listen = "127.0.0.1:8080"
p2p_listen = ["/ip4/127.0.0.1/tcp/4001"]

[storage]
backend = "memory"
capacity_mb = 1024

[federation]
bootstrap_period_sec = 5
peer_sync_interval_sec = 10
trust_bundle_sync_interval_sec = 30

[logging]
level = "debug"
output = "stdout"
EOF
                ;;
            testnet)
                cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-testnet-1"
mode = "validator"
data_dir = "./data"
max_memory_mib = 2048

[identity]
did = "did:icn:node:testnet1"
key_file = "./keys/node-key.pem"

[network]
http_listen = "0.0.0.0:8080"
p2p_listen = ["/ip4/0.0.0.0/tcp/4001"]
external_addresses = ["/ip4/127.0.0.1/tcp/4001"]

[storage]
backend = "filesystem"
base_dir = "./data/storage"
max_size_gb = 10

[federation]
bootstrap_peers = [
  "/ip4/testnet-bootstrap-1.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/testnet-bootstrap-2.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId2"
]

[logging]
level = "info"
output = "both"
file_path = "./logs/runtime.log"
EOF
                ;;
            production)
                cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-prod-1"
mode = "validator"
data_dir = "/var/lib/icn-runtime"
max_memory_mib = 4096

[identity]
did = "did:icn:node:prod1"
key_file = "/etc/icn-runtime/keys/node-key.pem"

[resources]
max_cpu_percent = 80
max_storage_gb = 100
max_concurrent_vms = 16

[network]
http_listen = "0.0.0.0:8080"
http_tls_enabled = true
http_tls_cert_file = "/etc/icn-runtime/tls/cert.pem"
http_tls_key_file = "/etc/icn-runtime/tls/key.pem"
p2p_listen = ["/ip4/0.0.0.0/tcp/4001"]
external_addresses = ["/dns4/node1.icn-example.org/tcp/4001"]
metrics_enabled = true
metrics_listen = "127.0.0.1:9090"

[storage]
backend = "filesystem"
base_dir = "/var/lib/icn-runtime/storage"
max_size_gb = 100
gc_interval_sec = 3600

[federation]
bootstrap_peers = [
  "/ip4/bootstrap-1.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/bootstrap-2.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId2"
]

[logging]
level = "info"
format = "json"
output = "both"
file_path = "/var/log/icn-runtime/runtime.log"
max_file_size_mb = 100
max_file_count = 10

[security]
sandbox_enabled = true
sandbox_type = "wasm"
access_control_enabled = true
EOF
                ;;
        esac
        echo -e "${GREEN}✓ Created configuration file: ${CONFIG_FILE}${NC}"
    fi
else
    # Generate appropriate configuration based on environment
    case "$DEPLOY_ENV" in
        development)
            cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-dev-1"
mode = "development"
data_dir = "./data"
max_memory_mib = 1024

[identity]
did = "did:icn:node:dev1"
key_file = "./keys/node-key.pem"

[network]
http_listen = "127.0.0.1:8080"
p2p_listen = ["/ip4/127.0.0.1/tcp/4001"]

[storage]
backend = "memory"
capacity_mb = 1024

[federation]
bootstrap_period_sec = 5
peer_sync_interval_sec = 10
trust_bundle_sync_interval_sec = 30

[logging]
level = "debug"
output = "stdout"
EOF
            ;;
        testnet)
            cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-testnet-1"
mode = "validator"
data_dir = "./data"
max_memory_mib = 2048

[identity]
did = "did:icn:node:testnet1"
key_file = "./keys/node-key.pem"

[network]
http_listen = "0.0.0.0:8080"
p2p_listen = ["/ip4/0.0.0.0/tcp/4001"]
external_addresses = ["/ip4/127.0.0.1/tcp/4001"]

[storage]
backend = "filesystem"
base_dir = "./data/storage"
max_size_gb = 10

[federation]
bootstrap_peers = [
  "/ip4/testnet-bootstrap-1.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/testnet-bootstrap-2.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId2"
]

[logging]
level = "info"
output = "both"
file_path = "./logs/runtime.log"
EOF
            ;;
        production)
            cat > ${CONFIG_FILE} << EOF
[runtime]
node_id = "icn-prod-1"
mode = "validator"
data_dir = "/var/lib/icn-runtime"
max_memory_mib = 4096

[identity]
did = "did:icn:node:prod1"
key_file = "/etc/icn-runtime/keys/node-key.pem"

[resources]
max_cpu_percent = 80
max_storage_gb = 100
max_concurrent_vms = 16

[network]
http_listen = "0.0.0.0:8080"
http_tls_enabled = true
http_tls_cert_file = "/etc/icn-runtime/tls/cert.pem"
http_tls_key_file = "/etc/icn-runtime/tls/key.pem"
p2p_listen = ["/ip4/0.0.0.0/tcp/4001"]
external_addresses = ["/dns4/node1.icn-example.org/tcp/4001"]
metrics_enabled = true
metrics_listen = "127.0.0.1:9090"

[storage]
backend = "filesystem"
base_dir = "/var/lib/icn-runtime/storage"
max_size_gb = 100
gc_interval_sec = 3600

[federation]
bootstrap_peers = [
  "/ip4/bootstrap-1.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId1",
  "/ip4/bootstrap-2.icn-example.org/tcp/4001/p2p/QmbootstrapNodeId2"
]

[logging]
level = "info"
format = "json"
output = "both"
file_path = "/var/log/icn-runtime/runtime.log"
max_file_size_mb = 100
max_file_count = 10

[security]
sandbox_enabled = true
sandbox_type = "wasm"
access_control_enabled = true
EOF
            ;;
    esac
    echo -e "${GREEN}✓ Created configuration file: ${CONFIG_FILE}${NC}"
fi

# Step 5: Generate key if needed
echo -e "${YELLOW}Step 5: Checking for node key...${NC}"

# Get key path from config
KEY_FILE=$(grep "key_file" ${CONFIG_FILE} | sed 's/key_file\s*=\s*"\(.*\)"/\1/')
KEY_DIR=$(dirname "$KEY_FILE")

if [ -f "$KEY_FILE" ]; then
    echo -e "${GREEN}✓ Key already exists: ${KEY_FILE}${NC}"
else
    echo "Key not found, generating new key at: ${KEY_FILE}"
    # Create directory if it doesn't exist
    mkdir -p "${KEY_DIR}"
    
    # Generate Ed25519 key with OpenSSL
    openssl genpkey -algorithm Ed25519 -out "${KEY_FILE}"
    chmod 600 "${KEY_FILE}"
    
    echo -e "${GREEN}✓ Generated new Ed25519 key: ${KEY_FILE}${NC}"
fi

# Step 6: Set up directory structure
echo -e "${YELLOW}Step 6: Setting up directory structure...${NC}"

# Extract data dir from config
DATA_DIR=$(grep "data_dir" ${CONFIG_FILE} | sed 's/data_dir\s*=\s*"\(.*\)"/\1/')

# Create directories if they don't exist
mkdir -p "${DATA_DIR}/storage"
mkdir -p "${DATA_DIR}/blobs"
mkdir -p "${DATA_DIR}/metadata"
mkdir -p "$(dirname $(grep "file_path" ${CONFIG_FILE} | sed 's/file_path\s*=\s*"\(.*\)"/\1/'))"

echo -e "${GREEN}✓ Created required directories${NC}"

# Step 7: Set appropriate permissions
echo -e "${YELLOW}Step 7: Setting file permissions...${NC}"

if [ "$DEPLOY_ENV" == "production" ]; then
    # For production, be more restrictive with permissions
    chmod 640 ${CONFIG_FILE}
    chmod 600 ${KEY_FILE}
    chmod 755 ${DATA_DIR}
    echo -e "${GREEN}✓ Set restrictive permissions for production${NC}"
else
    chmod 644 ${CONFIG_FILE}
    chmod 600 ${KEY_FILE}
    chmod 755 ${DATA_DIR}
    echo -e "${GREEN}✓ Set permissions${NC}"
fi

# Step 8: Create startup script
echo -e "${YELLOW}Step 8: Creating startup script...${NC}"
STARTUP_SCRIPT="${CONFIG_DIR}/start-${DEPLOY_ENV}.sh"

cat > ${STARTUP_SCRIPT} << EOF
#!/bin/bash
# ICN Runtime startup script for ${DEPLOY_ENV} environment

# Check for configuration file
if [ ! -f "${CONFIG_FILE}" ]; then
    echo "Error: Configuration file not found at ${CONFIG_FILE}"
    exit 1
fi

# Check for runtime binary
if [ ! -f "./target/release/icn-runtime" ]; then
    echo "Error: Runtime binary not found. Please build the project first."
    exit 1
fi

# Start the runtime
echo "Starting ICN Runtime with ${DEPLOY_ENV} configuration..."
./target/release/icn-runtime --config "${CONFIG_FILE}"
EOF

chmod +x ${STARTUP_SCRIPT}
echo -e "${GREEN}✓ Created startup script: ${STARTUP_SCRIPT}${NC}"

# Step 9: Create systemd service file (for production only)
if [ "$DEPLOY_ENV" == "production" ]; then
    echo -e "${YELLOW}Step 9: Creating systemd service file...${NC}"
    SERVICE_FILE="${CONFIG_DIR}/icn-runtime.service"
    
    cat > ${SERVICE_FILE} << EOF
[Unit]
Description=ICN Runtime Service
After=network.target

[Service]
Type=simple
User=icn-runtime
Group=icn-runtime
ExecStart=/usr/local/bin/icn-runtime --config /etc/icn-runtime/runtime-config-production.toml
Restart=on-failure
RestartSec=5
LimitNOFILE=65536

# Security settings
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
NoNewPrivileges=true
ReadWritePaths=/var/lib/icn-runtime /var/log/icn-runtime

[Install]
WantedBy=multi-user.target
EOF
    
    echo -e "${GREEN}✓ Created systemd service file: ${SERVICE_FILE}${NC}"
    echo "To install the service:"
    echo "  1. Copy ${SERVICE_FILE} to /etc/systemd/system/"
    echo "  2. Run: sudo systemctl daemon-reload"
    echo "  3. Run: sudo systemctl enable icn-runtime.service"
    echo "  4. Run: sudo systemctl start icn-runtime.service"
else
    echo -e "${YELLOW}Step 9: Skipping systemd service file (not needed for ${DEPLOY_ENV})${NC}"
fi

# Step 10: Installation instructions
echo
echo -e "${GREEN}==========================================${NC}"
echo -e "${GREEN}ICN Runtime preparation complete!${NC}"
echo -e "${GREEN}==========================================${NC}"
echo
echo -e "${YELLOW}Next steps:${NC}"
echo "  1. Review configuration in ${CONFIG_FILE}"
echo "  2. Start the runtime with: ${STARTUP_SCRIPT}"
echo
if [ "$DEPLOY_ENV" == "production" ]; then
    echo -e "${YELLOW}For production deployment:${NC}"
    echo "  1. Copy binary to /usr/local/bin:"
    echo "     sudo cp ./target/release/icn-runtime /usr/local/bin/"
    echo "  2. Copy configuration to /etc/icn-runtime:"
    echo "     sudo mkdir -p /etc/icn-runtime"
    echo "     sudo cp ${CONFIG_FILE} /etc/icn-runtime/runtime-config-production.toml"
    echo "  3. Copy key to /etc/icn-runtime/keys:"
    echo "     sudo mkdir -p /etc/icn-runtime/keys"
    echo "     sudo cp ${KEY_FILE} /etc/icn-runtime/keys/"
    echo "     sudo chmod 600 /etc/icn-runtime/keys/$(basename ${KEY_FILE})"
    echo "  4. Create runtime user:"
    echo "     sudo useradd -r -s /sbin/nologin icn-runtime"
    echo "  5. Create and set permissions on directories:"
    echo "     sudo mkdir -p /var/lib/icn-runtime /var/log/icn-runtime"
    echo "     sudo chown -R icn-runtime:icn-runtime /var/lib/icn-runtime /var/log/icn-runtime"
    echo "  6. Install and start systemd service (see instructions above)"
fi

echo
echo -e "${GREEN}Done!${NC}" 