#!/usr/bin/env bash
set -Eeuo pipefail
###############################################
#  ICN DEV‑NET ONE‑SHOT SPIN‑UP SCRIPT        #
#  – Runtime (CoVM v3)                        #
#  – AgoraNet                                 #
###############################################

REPOROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPOROOT/.."

echo "🔧 1. Building runtime components …"
cd runtime
cargo build --release -p icn-covm
cd ..

echo "💾 2. Launching Postgres for AgoraNet …"
# Stop and remove existing container if it exists
docker stop icn-pg >/dev/null 2>&1 || true
docker rm icn-pg >/dev/null 2>&1 || true
docker run --name icn-pg -e POSTGRES_PASSWORD=icnpass -e POSTGRES_USER=icn \
           -e POSTGRES_DB=agoranet -p 5432:5432 -d postgres:16-alpine
sleep 4        # give Postgres a moment

echo "📜 3. Running AgoraNet DB migrations …"
pushd agoranet >/dev/null
# Ensure sqlx-cli is installed or available
# If not installed globally, you might need: cargo install sqlx-cli --no-default-features --features native-tls,postgres
sqlx database create --database-url 'postgres://icn:icnpass@localhost:5432/agoranet' || true # Create DB if not exists, ignore error if it does
sqlx migrate run --database-url 'postgres://icn:icnpass@localhost:5432/agoranet'
popd >/dev/null

echo "🌱 4. Generating federation genesis & booting runtime …"
pushd runtime >/dev/null
# Create a genesis TrustBundle & DAG root
./target/release/covm \
     federation genesis --name dev-federation \
     --output genesis_trustbundle.json

# Start the runtime node (HTTP on :7000, gRPC on :7001)
# Kill existing process if any using the port
lsof -ti:7000 | xargs kill -9 >/dev/null 2>&1 || true
lsof -ti:7001 | xargs kill -9 >/dev/null 2>&1 || true
./target/release/covm \
     node start --config ./config/runtime-config-integration.toml \
     --genesis genesis_trustbundle.json --http 0.0.0.0:7000 --grpc 0.0.0.0:7001 \
     > ../runtime.log 2>&1 &
RUNTIME_PID=$!
popd >/dev/null
echo "   ↳ runtime PID: $RUNTIME_PID   (logs → runtime.log)"
# Wait a moment for the runtime to be ready
sleep 2

echo "🗣️ 5. Booting AgoraNet server …"
pushd agoranet >/dev/null
# Kill existing process if any using the port
lsof -ti:3001 | xargs kill -9 >/dev/null 2>&1 || true
cargo run --release -- \
     --db-url 'postgres://icn:icnpass@localhost:5432/agoranet' \
     --runtime-url 'http://localhost:7000' \
     --listen 0.0.0.0:3001 \
     > ../agoranet.log 2>&1 &
AGORANET_PID=$!
popd >/dev/null
echo "   ↳ agoranet PID: $AGORANET_PID (logs → agoranet.log)"
# Wait a moment for AgoraNet to be ready
sleep 2

# Trap SIGINT (Ctrl+C) to kill background processes
trap 'echo "🛑 Shutting down background processes..."; kill $RUNTIME_PID $AGORANET_PID; exit' INT

echo ""
echo "✅ ICN dev‑net is now live!"
echo "   • Runtime API  : http://localhost:7000"
echo "   • AgoraNet API : http://localhost:3001"
echo ""
echo "Press Ctrl+C to stop the devnet servers."

# Wait for Ctrl+C
wait

# This part might not be reached if Ctrl+C is used to stop the servers
echo ""
echo "✅ ICN dev‑net was stopped."
echo "Runtime PID ($RUNTIME_PID) and AgoraNet PID ($AGORANET_PID) have been terminated." 