#[no_mangle]
pub extern "C" fn _start() {
    log_message(1, "Test WASM executed successfully!");
}

#[link(wasm_import_module = "env")]
extern "C" {
    #[link_name = "host_log_message"]
    fn log_raw(level: i32, ptr: i32, len: i32);
}

fn log_message(level: i32, message: &str) {
    unsafe {
        let ptr = message.as_ptr() as i32;
        let len = message.len() as i32;
        log_raw(level, ptr, len);
    }
}
