#!/usr/bin/env bash
set -Eeuo pipefail
###############################################
#  ICN DEV‑NET ONE‑SHOT SPIN‑UP SCRIPT        #
#  – Runtime (CoVM v3)                        #
#  – AgoraNet                                 #
###############################################

# Configuration
DEBUG_MODE=false
BUILD_MODE="debug"
POSTGRESQL_PORT=5432

# Parse command line arguments
for arg in "$@"; do
  case $arg in
    --debug)
      DEBUG_MODE=true
      shift
      ;;
    --release)
      BUILD_MODE="release"
      shift
      ;;
    --help)
      echo "Usage: $0 [--debug] [--release]"
      echo ""
      echo "Options:"
      echo "  --debug    Enable debug mode with more verbose output"
      echo "  --release  Build in release mode (default is debug mode)"
      exit 0
      ;;
  esac
done

# Set up logging
log() {
  local level="$1"
  local msg="$2"
  local timestamp=$(date +"%Y-%m-%d %H:%M:%S")
  echo "[$timestamp] [$level] $msg"
}

info() {
  log "INFO" "$1"
}

error() {
  log "ERROR" "$1"
  exit 1
}

debug() {
  if [[ "$DEBUG_MODE" == true ]]; then
    log "DEBUG" "$1"
  fi
}

# Check prerequisites
check_prerequisites() {
  info "Checking prerequisites..."
  
  # Check for Docker
  if ! command -v docker &> /dev/null; then
    error "Docker is not installed or not in PATH"
  fi
  
  # Check if Docker is running
  if ! docker info &> /dev/null; then
    error "Docker daemon is not running"
  fi
  
  # Check for Rust/Cargo
  if ! command -v cargo &> /dev/null; then
    error "Cargo is not installed or not in PATH"
  fi
  
  # Check available disk space (need at least 5GB)
  local available_space=$(df -k . | awk 'NR==2 {print $4}')
  if [[ $available_space -lt 5000000 ]]; then
    error "Not enough disk space. At least 5GB required, $(($available_space/1000000))GB available"
  fi
  
  # Check if PostgreSQL port is already in use
  if netstat -tuln 2>/dev/null | grep -q ":$POSTGRESQL_PORT "; then
    error "Port $POSTGRESQL_PORT is already in use. Please free it before continuing."
  fi
  
  debug "All prerequisites satisfied"
}

# Build ICN runtime components
build_runtime() {
  info "Building runtime components in $BUILD_MODE mode..."
  
  cd icn-runtime-root/cli
  if [[ "$DEBUG_MODE" == true ]]; then
    cargo build --verbose
  else
    if [[ "$BUILD_MODE" == "release" ]]; then
      cargo build --release
    else
      cargo build
    fi
  fi
  
  if [[ $? -ne 0 ]]; then
    error "Failed to build runtime components"
  fi
  
  cd ../..
  info "Runtime build successful"
}

# Set up PostgreSQL for AgoraNet
setup_postgres() {
  info "Launching Postgres for AgoraNet..."
  
  # Stop and remove existing container if it exists
  docker stop icn-pg >/dev/null 2>&1 || true
  docker rm icn-pg >/dev/null 2>&1 || true
  
  # Start PostgreSQL
  docker run --name icn-pg -e POSTGRES_PASSWORD=icnpass -e POSTGRES_USER=icn \
           -e POSTGRES_DB=agoranet -p $POSTGRESQL_PORT:5432 -d postgres:16-alpine
  
  if [[ $? -ne 0 ]]; then
    error "Failed to start PostgreSQL container"
  fi
  
  # Wait for PostgreSQL to be ready
  info "Waiting for PostgreSQL to be ready..."
  sleep 5
  
  # Test connection to PostgreSQL
  for i in {1..10}; do
    if docker exec icn-pg pg_isready -U icn -d agoranet > /dev/null 2>&1; then
      info "PostgreSQL is ready"
      break
    fi
    if [[ $i -eq 10 ]]; then
      error "PostgreSQL failed to start"
    fi
    sleep 2
  done
}

# Run AgoraNet migrations
run_migrations() {
  info "Running AgoraNet DB migrations..."
  
  cd agoranet
  if ! cargo run -- migrate up; then
    error "Failed to run AgoraNet migrations"
  fi
  cd ..
  
  info "Migrations successful"
}

# Initialize runtime test environment
init_runtime() {
  info "Initializing ICN Runtime environment..."
  
  mkdir -p data/runtime
  
  TARGET_DIR="./target"
  if [[ "$BUILD_MODE" == "release" ]]; then
    BINARY="$TARGET_DIR/release/covm"
  else
    BINARY="$TARGET_DIR/debug/covm"
  fi
  
  if [[ ! -f "$BINARY" ]]; then
    error "Runtime binary not found at $BINARY"
  fi
  
  # Start runtime process in background
  $BINARY serve --database-url postgresql://icn:icnpass@localhost:5432/agoranet > runtime.log 2>&1 &
  RUNTIME_PID=$!
  
  info "Runtime started with PID $RUNTIME_PID"
  
  # Check if runtime started successfully
  sleep 3
  if ! ps -p $RUNTIME_PID > /dev/null; then
    error "Runtime failed to start. Check runtime.log for details"
  fi
  
  info "Runtime successfully initialized"
}

# Main execution
main() {
  info "Starting ICN Development Network..."
  
  check_prerequisites
  build_runtime
  setup_postgres
  run_migrations
  init_runtime
  
  info "ICN Development Network is ready!"
  info "Runtime logs are in runtime.log"
  info "To stop the network, run: docker stop icn-pg && kill $RUNTIME_PID"
}

# Run the main function
main 