# ICN Component Export Scripts

This directory contains scripts for exporting individual components of the ICN monorepo into standalone repositories for deployment or contributor onboarding.

## Available Export Scripts

| Script | Purpose | Output Location |
|--------|---------|-----------------|
| `export-runtime.sh` | Extracts the Runtime component | `<repo-root>/export/icn-runtime` |
| `export-wallet.sh` | Extracts the Wallet component | `<repo-root>/export/icn-wallet` |
| `export-agoranet.sh` | Extracts the AgoraNet component | `<repo-root>/export/icn-agoranet` |

## Usage

Each script can be run directly from the scripts directory:

```bash
# Export the Runtime component
./export-runtime.sh

# Export the Wallet component
./export-wallet.sh

# Export the AgoraNet component
./export-agoranet.sh
```

## What the Scripts Do

The export scripts perform the following operations:

1. Create a temporary directory structure
2. Copy the relevant component files from the monorepo
3. Copy any shared dependencies needed by the component
4. Create a new root `Cargo.toml` workspace file
5. Generate README files, CI workflows, and other supporting files
6. Move the temporary directory to the final output location

## After Export

After exporting a component, you can:

1. Navigate to the exported directory:
   ```bash
   cd ../export/icn-wallet
   ```

2. Initialize a new Git repository:
   ```bash
   git init
   git add .
   git commit -m "Initial export from monorepo"
   ```

3. Push to a new remote repository:
   ```bash
   git remote add origin <your-repo-url>
   git push -u origin main
   ```

## Component Dependencies

- **Runtime**: Depends on core DAG functionality
- **Wallet**: Depends on wallet-types and dag-core
- **AgoraNet**: Depends on dag-core for proposal linking

Each export script will automatically include the required dependencies from the monorepo.

## Customizing Exports

If you need to modify what gets exported:

1. Edit the relevant export script
2. Adjust the file copying logic as needed
3. Update the generated Cargo.toml to include any additional dependencies

## Troubleshooting

If you encounter issues with an export:

- Check that all required dependencies are included
- Verify that the workspace configuration is correct
- Ensure that the proper directory structure is maintained

For further assistance, refer to the ICN development documentation. 