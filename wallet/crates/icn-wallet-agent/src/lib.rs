/*!
 * ICN Wallet Agent
 *
 * Command-line and API interface for ICN wallet operations including
 * receipt import, verification, and management.
 */

pub mod import;
pub mod cli;
pub mod share;

pub use import::{import_receipts_from_file, ImportError, ExecutionReceipt};
pub use share::{
    share_receipts, share_receipts_as_json, share_receipts_as_bundle, 
    share_receipts_as_encrypted_bundle, ShareOptions, ShareFormat, ShareError,
    encrypt_receipt_bundle, decrypt_receipt_bundle, EncryptedBundle,
    generate_share_link, generate_share_link_from_receipts
};
pub use cli::run_cli; 