pub mod identity;
pub mod crypto;
pub mod error;
pub mod credential;
pub mod vc;
pub mod dag;
pub mod store;

pub use identity::IdentityWallet;
pub use credential::CredentialSigner;
pub use vc::{VerifiableCredential, VerifiablePresentation};
