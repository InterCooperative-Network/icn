use std::fs;
use std::io::Write;

fn main() {
    // Tell Cargo to re-run this build script if the UDL file changes
    println!("cargo:rerun-if-changed=src/minimal.udl");
    println!("cargo:rerun-if-env-changed=UNIFFI_TESTS_DISABLE_EXTENSIONS");

    // Create the UDL content manually - simplest possible form that works
    let udl_content = "namespace wallet_minimal {};";
    let udl_path = "src/minimal.udl";
    
    // Write the UDL content to the file
    fs::write(udl_path, udl_content).expect("Failed to write UDL file");
    
    println!("UDL file written: {}", udl_content);

    // Generate FFI bindings from UDL file
    match uniffi_build::generate_scaffolding(udl_path) {
        Ok(_) => println!("Successfully generated scaffolding"),
        Err(e) => {
            eprintln!("Failed to generate scaffolding: {:?}", e);
            panic!("Failed to generate scaffolding: {:?}", e);
        }
    }
} 