#!/usr/bin/env bash
# Script to verify migrations can be applied sequentially to a fresh database

set -e

# Check if DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
  echo "DATABASE_URL environment variable must be set"
  exit 1
fi

# Create a temporary database for testing
DB_NAME="icn_migration_test_$(date +%s)"
echo "Creating temporary database: $DB_NAME"

# Extract the base connection string without the database
BASE_URL=$(echo $DATABASE_URL | sed -E 's|/[^/]+$|/postgres|')

echo "Creating test database..."
PGPASSWORD=$PGPASSWORD psql "$BASE_URL" -c "CREATE DATABASE $DB_NAME;"

# Set the test database URL
TEST_DB_URL=$(echo $DATABASE_URL | sed -E "s|/[^/]+$|/$DB_NAME|")

# Run migrations
echo "Running migrations..."
DATABASE_URL=$TEST_DB_URL sqlx database create
DATABASE_URL=$TEST_DB_URL sqlx migrate info
DATABASE_URL=$TEST_DB_URL sqlx migrate run

# Check if migrations were successful
if [ $? -eq 0 ]; then
  echo "Migrations applied successfully!"
else
  echo "Migration check failed!"
  # Clean up the test database even on failure
  PGPASSWORD=$PGPASSWORD psql "$BASE_URL" -c "DROP DATABASE $DB_NAME;"
  exit 1
fi

# Verify tables exist
echo "Verifying tables..."
TABLE_COUNT=$(PGPASSWORD=$PGPASSWORD psql -t "$TEST_DB_URL" -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';")

echo "Found $TABLE_COUNT tables"

if [ $TABLE_COUNT -lt 5 ]; then
  echo "Expected at least 5 tables to be created by migrations"
  PGPASSWORD=$PGPASSWORD psql "$BASE_URL" -c "DROP DATABASE $DB_NAME;"
  exit 1
fi

# Clean up the test database
echo "Cleaning up test database..."
PGPASSWORD=$PGPASSWORD psql "$BASE_URL" -c "DROP DATABASE $DB_NAME;"

echo "Migration validation completed successfully!"
exit 0 