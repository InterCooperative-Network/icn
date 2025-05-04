#!/bin/bash
# Script to securely move private keys out of the repository
# and into a protected secrets directory

set -e

REPO_DIR=$(git rev-parse --show-toplevel)
KEYS_DIR="$REPO_DIR/wallet/.keys"
SECRETS_DIR="$REPO_DIR/secrets/keys"

# Create the secrets directory if it doesn't exist
mkdir -p "$SECRETS_DIR"

# Check if there are any keys to move
if [ ! "$(ls -A $KEYS_DIR)" ]; then
    echo "No key files found in $KEYS_DIR"
    exit 0
fi

# Move keys to secrets directory
echo "Moving keys from $KEYS_DIR to $SECRETS_DIR"
cp -v "$KEYS_DIR"/*.json "$SECRETS_DIR"/ 2>/dev/null || true

# Replace original keys with placeholder files
for file in "$KEYS_DIR"/*.json; do
    if [ -f "$file" ]; then
        filename=$(basename "$file")
        # Create a placeholder file
        echo "{\"note\":\"This is a placeholder. Actual key stored in secrets/keys/$filename\"}" > "$file"
        echo "Replaced $file with placeholder"
    fi
done

# Secure the secrets directory
chmod -R 700 "$SECRETS_DIR"

echo "Done! Keys have been securely moved to $SECRETS_DIR"
echo "You can now safely commit the repository without exposing private keys."
echo "Remember to add secrets/ to your .gitignore file and never commit it." 