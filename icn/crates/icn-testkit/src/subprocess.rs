//! Subprocess helpers for Layer 4 cross-process persistence proofs.
//!
//! These helpers eliminate the repeated boilerplate in Layer 4 integration
//! tests that spawn helper binaries to prove cross-process durability.
//!
//! See `docs/development/testing/verification-pattern.md` for the full
//! 4-layer verification pattern and how to use these helpers.

/// Run a helper binary with the given arguments and assert that it exits 0.
///
/// Returns the trimmed stdout string for further parsing.
///
/// On failure, panics with a detailed message including the command,
/// exit code, full stdout, and full stderr — making test failures
/// immediately actionable in CI logs.
///
/// # Usage
///
/// ```rust,ignore
/// use icn_testkit::subprocess::run_subprocess;
///
/// let helper = env!("CARGO_BIN_EXE_my_restart_helper");
/// let data_dir = tmp.path().to_str().unwrap();
///
/// // Write phase: returns "token_a,token_b"
/// let write_stdout = run_subprocess(helper, &["write", data_dir]);
/// let (token_a, token_b) = write_stdout.split_once(',').expect("expected 'a,b'");
///
/// // Read phase: asserts exact invariants inside the subprocess
/// run_subprocess(helper, &["read", data_dir, token_a, token_b]);
/// ```
#[allow(clippy::panic)]
pub fn run_subprocess(binary: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(binary)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `{binary}`: {e}"));

    assert!(
        output.status.success(),
        "subprocess failed\ncmd: {binary} {args}\nexit: {code:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args = args.join(" "),
        code = output.status.code(),
        stdout = String::from_utf8_lossy(&output.stdout),
        stderr = String::from_utf8_lossy(&output.stderr),
    );

    String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("subprocess stdout from `{binary}` was not valid UTF-8"))
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_subprocess_succeeds_and_returns_stdout() {
        // `true` is a standard Unix binary that exits 0 with no output.
        let out = run_subprocess("true", &[]);
        assert_eq!(out, "");
    }

    #[test]
    #[should_panic(expected = "subprocess failed")]
    fn run_subprocess_panics_on_nonzero_exit() {
        // `false` exits 1.
        run_subprocess("false", &[]);
    }
}
