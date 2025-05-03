fn main() {
    // Tell Cargo to re-run this build script if the UDL file changes
    println!("cargo:rerun-if-changed=src/wallet.udl");
    
    // Generate FFI bindings from UDL file
    uniffi_build::generate_scaffolding("src/wallet.udl")
        .expect("Failed to generate bindings from UDL");
} 