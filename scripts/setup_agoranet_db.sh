#!/bin/bash
set -e

echo "Setting up AgoraNet database..."

# Database configuration
DB_HOST=${PGHOST:-localhost}
DB_PORT=${PGPORT:-5432}
DB_USER=${PGUSER:-postgres}
DB_PASSWORD=${PGPASSWORD:-postgres}
DB_NAME=${PGDATABASE:-agoranet}

# Export DATABASE_URL for sqlx
export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"

# Check if the database exists
if ! PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U $DB_USER -lqt | cut -d \| -f 1 | grep -qw $DB_NAME; then
    echo "Creating database $DB_NAME..."
    PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U $DB_USER -c "CREATE DATABASE $DB_NAME;"
    echo "Database created."
else
    echo "Database $DB_NAME already exists."
fi

# Create .env file if it doesn't exist
if [ ! -f /home/matt/dev/icn/agoranet/.env ]; then
    echo "Creating .env file..."
    cat > /home/matt/dev/icn/agoranet/.env << EOF
# Database configuration
DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}
PGUSER=${DB_USER}
PGPASSWORD=${DB_PASSWORD}
PGDATABASE=${DB_NAME}
PGHOST=${DB_HOST}
PGPORT=${DB_PORT}

# API configuration
PORT=3030
HOST=0.0.0.0

# Federation configuration
FEDERATION_ID=dev_federation
NODE_ID=dev_node
PEER_DISCOVERY_INTERVAL=30
SYNC_INTERVAL=60

# Log configuration
RUST_LOG=info,agoranet=debug
EOF
    echo ".env file created."
else
    echo ".env file already exists."
fi

# Run the migrations
echo "Running database migrations..."
cd /home/matt/dev/icn/agoranet
cargo sqlx migrate run
echo "Migrations completed."

# Generate sqlx-data.json for offline use (optional)
if [ "$1" == "--prepare" ]; then
    echo "Generating sqlx-data.json for offline mode..."
    cargo sqlx prepare --database-url $DATABASE_URL
    echo "sqlx-data.json generated."
fi

echo "AgoraNet database setup complete!" 