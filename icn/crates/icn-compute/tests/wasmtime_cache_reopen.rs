//! Characterize Wasmtime's optional persisted compilation cache on ICN's locked graph.
//!
//! ICN's WasmExecutor currently uses Engine::default(), whose disk cache is disabled.
//! These tests opt in only for private temporary directories. They exercise Module::new,
//! including validation, compilation, compressed artifact persistence and native loading;
//! they do not call unsafe Module::deserialize or trust arbitrary native-code artifacts.
#![cfg(feature = "wasm")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wasmtime::{Cache, CacheConfig, Config, Engine, Linker, Module, OptLevel, Store};

const WASM: &str = r#"(module (func (export "run") (result i32) i32.const 42))"#;

fn cached_engine(root: &Path, unoptimized: bool) -> (Engine, Cache) {
    let mut cache_config = CacheConfig::new();
    cache_config.with_directory(root);
    let cache = Cache::new(cache_config).unwrap();
    let mut config = Config::new();
    config.cache(Some(cache.clone()));
    if unoptimized {
        config.cranelift_opt_level(OptLevel::None);
    }
    (Engine::new(&config).unwrap(), cache)
}

fn load_and_run(root: &Path, unoptimized: bool, expected: (usize, usize)) {
    let (engine, cache) = cached_engine(root, unoptimized);
    assert_eq!((cache.cache_hits(), cache.cache_misses()), (0, 0));
    let module = Module::new(&engine, WASM).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate(&mut store, &module)
        .unwrap();
    let result = instance
        .get_typed_func::<(), i32>(&mut store, "run")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    assert_eq!(result, 42);
    // These counters are synchronous and only count successful artifact loads/writes.
    // A hit is incremented after Wasmtime accepts the decompressed compiled artifact.
    assert_eq!((cache.cache_hits(), cache.cache_misses()), expected);
    println!("result={result} hits={} misses={}", expected.0, expected.1);
    // Store, module, engine, Config's cache clone and our cache all die on return.
}

fn entries(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let modules = root.join("modules");
    if !modules.exists() {
        return files;
    }
    for compiler in fs::read_dir(modules).unwrap() {
        for entry in fs::read_dir(compiler.unwrap().path()).unwrap() {
            let path = entry.unwrap().path();
            // Worker stats and in-progress writes have extensions; module keys do not.
            if path.is_file() && path.extension().is_none() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[test]
fn cold_cache_reopens_with_a_fresh_engine() {
    let root = TempDir::new().unwrap();
    assert!(entries(root.path()).is_empty());
    load_and_run(root.path(), false, (0, 1));
    let paths = entries(root.path());
    assert_eq!(paths.len(), 1);
    let compressed = fs::read(&paths[0]).unwrap();
    assert!(compressed.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]));
    load_and_run(root.path(), false, (1, 0));
    assert_eq!(entries(root.path()), paths);
}

#[test]
fn cache_reopens_in_a_new_process() {
    const ROOT_ENV: &str = "ICN_WASMTIME_CACHE_TEST_ROOT";
    const STEP_ENV: &str = "ICN_WASMTIME_CACHE_TEST_STEP";
    if let Some(root) = std::env::var_os(ROOT_ENV) {
        let expected = match std::env::var(STEP_ENV).unwrap().as_str() {
            "cold" => (0, 1),
            "warm" => (1, 0),
            step => panic!("unknown cache test step {step}"),
        };
        load_and_run(Path::new(&root), false, expected);
        return;
    }

    let root = TempDir::new().unwrap();
    for step in ["cold", "warm"] {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cache_reopens_in_a_new_process", "--nocapture"])
            .env(ROOT_ENV, root.path())
            .env(STEP_ENV, step)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{step} cache subprocess failed: {status}");
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{step} cache subprocess exceeded 30 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(entries(root.path()).len(), 1);
    }
}

#[test]
fn invalid_cache_entries_recompile_then_reopen() {
    // Both failure stages must fall back: zstd decoding and compiled-object loading.
    // This valid zstd frame contains the literal "not a Wasmtime compiled module".
    const NON_OBJECT: &[u8] = &[
        40, 181, 47, 253, 32, 30, 241, 0, 0, 110, 111, 116, 32, 97, 32, 87, 97, 115, 109, 116, 105,
        109, 101, 32, 99, 111, 109, 112, 105, 108, 101, 100, 32, 109, 111, 100, 117, 108, 101,
    ];
    for invalid in [b"invalid zstd cache entry".as_slice(), NON_OBJECT] {
        let root = TempDir::new().unwrap();
        load_and_run(root.path(), false, (0, 1));
        let paths = entries(root.path());
        assert_eq!(paths.len(), 1);
        fs::write(&paths[0], invalid).unwrap();
        load_and_run(root.path(), false, (0, 1));
        assert_ne!(fs::read(&paths[0]).unwrap(), invalid);
        load_and_run(root.path(), false, (1, 0));
    }
}

#[test]
fn compilation_settings_partition_cache_entries() {
    let root = TempDir::new().unwrap();
    load_and_run(root.path(), false, (0, 1));
    load_and_run(root.path(), true, (0, 1));
    assert_eq!(entries(root.path()).len(), 2);
    load_and_run(root.path(), false, (1, 0));
    load_and_run(root.path(), true, (1, 0));
}

#[test]
fn cache_does_not_accept_invalid_wasm() {
    let root = TempDir::new().unwrap();
    load_and_run(root.path(), false, (0, 1));
    let before = entries(root.path());
    let (engine, cache) = cached_engine(root.path(), false);
    assert!(Module::from_binary(&engine, b"\0asm\x01\0\0\0\xff").is_err());
    assert_eq!((cache.cache_hits(), cache.cache_misses()), (0, 0));
    assert_eq!(entries(root.path()), before);
}
