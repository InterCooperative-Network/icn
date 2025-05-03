#!/bin/bash
# wait_for_services.sh
#
# This script waits for services defined in docker-compose to be healthy
# before proceeding with tests or other operations.

set -e

# Default values
TIMEOUT=120
INTERVAL=5
COMPOSE_FILE="../docker-compose.integration.yml"
SERVICES=()

# Function to display usage information
show_usage() {
  echo "Usage: $0 [options]"
  echo "Wait for Docker services to be healthy"
  echo ""
  echo "Options:"
  echo "  -h, --help                 Display this help message"
  echo "  -t, --timeout <seconds>    Maximum time to wait (default: 120)"
  echo "  -i, --interval <seconds>   Check interval (default: 5)"
  echo "  -f, --file <path>          Docker compose file path (default: ../docker-compose.integration.yml)"
  echo "  -s, --service <name>       Specific service to wait for (can be used multiple times)"
  echo ""
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      show_usage
      exit 0
      ;;
    -t|--timeout)
      TIMEOUT="$2"
      shift 2
      ;;
    -i|--interval)
      INTERVAL="$2"
      shift 2
      ;;
    -f|--file)
      COMPOSE_FILE="$2"
      shift 2
      ;;
    -s|--service)
      SERVICES+=("$2")
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      show_usage
      exit 1
      ;;
  esac
done

# Ensure Docker Compose file exists
if [ ! -f "$COMPOSE_FILE" ]; then
  echo "Error: Docker Compose file not found: $COMPOSE_FILE"
  exit 1
fi

# Get all services if none specified
if [ ${#SERVICES[@]} -eq 0 ]; then
  echo "No specific services specified, checking all services..."
  SERVICES=($(docker-compose -f "$COMPOSE_FILE" ps --services))
fi

echo "Waiting for services: ${SERVICES[*]}"
echo "Timeout: ${TIMEOUT}s, Check interval: ${INTERVAL}s"

start_time=$(date +%s)
end_time=$((start_time + TIMEOUT))

# Wait for each service
for service in "${SERVICES[@]}"; do
  echo "Waiting for service: $service"
  
  while true; do
    current_time=$(date +%s)
    
    if [ $current_time -gt $end_time ]; then
      echo "Error: Timeout waiting for service: $service"
      exit 1
    fi
    
    # Check if the service is running
    status=$(docker-compose -f "$COMPOSE_FILE" ps -q "$service" 2>/dev/null)
    if [ -z "$status" ]; then
      echo "Service $service is not running"
      sleep $INTERVAL
      continue
    fi
    
    # Check service health status
    health=$(docker inspect --format='{{.State.Health.Status}}' $(docker-compose -f "$COMPOSE_FILE" ps -q "$service") 2>/dev/null || echo "unknown")
    
    # If service doesn't have health check, consider it ready
    if [ "$health" = "unknown" ]; then
      echo "Service $service does not have a health check, assuming it's ready"
      break
    elif [ "$health" = "healthy" ]; then
      echo "Service $service is healthy"
      break
    else
      elapsed=$((current_time - start_time))
      echo "Service $service is not yet healthy (status: $health), waiting... [${elapsed}s elapsed]"
      sleep $INTERVAL
    fi
  done
done

total_time=$(($(date +%s) - start_time))
echo "All services are ready after ${total_time}s"
exit 0 