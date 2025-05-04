fn main() {
    // Generate UniFFI bindings when the crate is built
    uniffi::generate_scaffolding("src/wallet.udl").unwrap();
} 