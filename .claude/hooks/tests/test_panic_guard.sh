#!/bin/bash
# Regression tests for .claude/hooks/panic-guard.sh.
#
# Feeds synthetic PreToolUse payloads on stdin and checks the hook's whole output contract:
#   exit 2 + reason on stderr + empty stdout  -> blocked
#   exit 0 + empty stdout                     -> allowed silently
#   exit 0 + exactly ONE valid JSON object    -> allowed with a warning
# No repository file is read or written: every path named in a payload is a fictional path under a
# fresh temporary directory, and nothing is created there except the hook's own captured output.
#
# Usage: test_panic_guard.sh [path/to/panic-guard.sh]   (default: the sibling ../panic-guard.sh)
set -u

HOOK=${1:-"$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/panic-guard.sh"}
command -v jq >/dev/null 2>&1 || { echo "jq is required to run these tests" >&2; exit 2; }
[[ -f "$HOOK" ]] || { echo "hook not found: $HOOK" >&2; exit 2; }

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
SRC="$T/repo/crates/x/src"
PASS=0
FAIL=0

# run <stdin>: runs the hook, setting RC, OUT, ERR.
run() {
  printf '%s' "$1" | bash "$HOOK" >"$T/out" 2>"$T/err"
  RC=$?
  OUT=$(cat "$T/out")
  ERR=$(cat "$T/err")
}

edit() { jq -nc --arg p "$1" --arg s "$2" '{tool_name:"Edit",tool_input:{file_path:$p,old_string:"x",new_string:$s}}'; }
write() { jq -nc --arg p "$1" --arg c "$2" '{tool_name:"Write",tool_input:{file_path:$p,content:$c}}'; }

ok() { PASS=$((PASS + 1)); printf 'ok    %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf 'FAIL  %s: %s (rc=%s stdout=%q stderr=%q)\n' "$1" "$2" "$RC" "$OUT" "${ERR:0:120}"; }

expect_silent() { # name payload
  run "$2"
  if [[ $RC -ne 0 ]]; then bad "$1" "expected exit 0"
  elif [[ -n "$OUT" ]]; then bad "$1" "expected empty stdout"
  else ok "$1"; fi
}

expect_block() { # name payload
  run "$2"
  if [[ $RC -ne 2 ]]; then bad "$1" "expected exit 2 (the only blocking status)"
  elif [[ -n "$OUT" ]]; then bad "$1" "expected empty stdout on block"
  elif [[ "$ERR" != *"PANIC GUARD VIOLATION"* ]]; then bad "$1" "expected the reason on stderr"
  else ok "$1"; fi
}

# expect_warn name payload expected-path needle...: exactly one JSON object, continue=true, whose
# systemMessage names the exact path and contains every needle.
expect_warn() {
  local name=$1 payload=$2 path=$3
  shift 3
  run "$payload"
  if [[ $RC -ne 0 ]]; then bad "$name" "expected exit 0"; return; fi
  local n
  n=$(printf '%s' "$OUT" | jq -s 'length' 2>/dev/null) || { bad "$name" "stdout is not valid JSON"; return; }
  if [[ "$n" != 1 ]]; then bad "$name" "expected exactly one JSON object, got $n"; return; fi
  if ! printf '%s' "$OUT" | jq -e 'type == "object" and .continue == true and (.systemMessage | type) == "string"' >/dev/null; then
    bad "$name" "object lacks continue:true / string systemMessage"; return
  fi
  local msg
  msg=$(printf '%s' "$OUT" | jq -r '.systemMessage')
  if [[ "$msg" != "Panic guard warning in $path:"* ]]; then bad "$name" "message does not name the exact path"; return; fi
  local needle
  for needle in "$@"; do
    if [[ "$msg" != *"$needle"* ]]; then bad "$name" "message lacks: $needle"; return; fi
  done
  ok "$name"
}

P="$SRC/lib.rs"
UNWRAP='.unwrap() found'
EXPECT='.expect() found'
PANIC='panic!() in snippet'

echo "# allowed edits"
expect_silent "non-Rust file"                     "$(write "$T/repo/README.md" 'panic!("x") a.unwrap()')"
expect_silent "clean Rust edit"                   "$(edit "$P" 'fn f() -> Result<(), E> { Ok(()) }')"
expect_silent "clean Rust write"                  "$(write "$P" 'pub fn f() -> u8 { 1 }')"
expect_silent "edit with no new content"          "$(jq -nc --arg p "$P" '{tool_name:"Edit",tool_input:{file_path:$p,old_string:"x",new_string:""}}')"
expect_silent "unwrap in cfg(test) snippet"       "$(edit "$P" $'#[cfg(test)]\nmod t { fn f() { a.unwrap(); } }')"
expect_silent "allow(clippy::unwrap_used)"        "$(edit "$P" $'#[allow(clippy::unwrap_used)]\nlet a = b.unwrap();')"
expect_silent "panic only inside #[cfg(test)]"    "$(write "$P" $'pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { panic!("boom") }\n}\n')"
expect_silent "panic in // and /// comments"      "$(write "$P" $'/// Never panic!("x") here.\n// panic!("old")\npub fn f() {}\n')"
expect_silent "should_panic / my_panic! / panics" "$(write "$P" $'#[should_panic]\nfn a() { my_panic!(1); }\nfn panics() {}\n')"
expect_silent "allow(clippy::panic) item"         "$(write "$P" $'#[allow(clippy::panic)]\npub fn spawn() { x.unwrap_or_else(|e| panic!("{e}")); }\npub fn g() {}\n')"

echo "# individual and combined warnings"
expect_warn "unwrap only"                "$(edit "$P" 'let a = b.unwrap();')"               "$P" "$UNWRAP" 'expect("reason")'
expect_warn "expect only"                "$(edit "$P" 'let a = b.expect("y");')"            "$P" "$EXPECT"
expect_warn "panic snippet only (Edit)"  "$(edit "$P" 'panic!("x")')"                       "$P" "$PANIC"
expect_warn "panic + unwrap + expect"    "$(edit "$P" 'panic!("x"); b.unwrap(); c.expect("z");')" "$P" "$PANIC" "$UNWRAP" "$EXPECT"
expect_warn "prod panic beside test mod (Edit)" "$(edit "$P" $'fn f() { panic!("p") }\n#[cfg(test)]\nmod t {}')" "$P" "$PANIC"
expect_warn "quotes and backslashes in code" "$(edit "$P" $'let s = "a\\"b\\\\c"; d.unwrap();')" "$P" "$UNWRAP"

echo "# unusual file paths (escaped, never interpolated)"
for q in "$SRC/a\"b/lib.rs" "$SRC/a\\b/lib.rs" "$SRC/sp ace/ünï/lib.rs" "$SRC/tab	x/lib.rs" "$SRC"$'/new\nline/lib.rs' "$SRC/\$(touch $T/pwned)/\`touch $T/pwned\`.rs" "$SRC/brace{}\"]}/lib.rs"; do
  expect_warn "path $(printf '%q' "$q" | cut -c1-60)" "$(edit "$q" 'b.unwrap();')" "$q" "$UNWRAP"
done
if [[ -e "$T/pwned" ]]; then FAIL=$((FAIL + 1)); echo "FAIL  a file path was executed"; else ok "file paths are never executed"; fi

echo "# legitimate test files"
for tp in "$T/repo/crates/x/tests/it.rs" "$T/repo/crates/icn-compute/src/actor/tests.rs" "$SRC/tests.rs" "$SRC/foo_test.rs" "$T/repo/crates/x/src/test_utils/mod.rs"; do
  expect_silent "test path ${tp#"$T"/repo/}" "$(write "$tp" 'pub fn f() { panic!("x") }')"
done

echo "# production panic violations (Write blocks)"
expect_block "prod panic"                          "$(write "$P" 'pub fn f() { panic!("x") }')"
expect_block "prod panic + test mod with panic"    "$(write "$P" $'pub fn f() { panic!("prod") }\n#[cfg(test)]\nmod tests { fn t() { panic!("t") } }\n')"
expect_block "prod panic AFTER a test mod"         "$(write "$P" $'#[cfg(test)]\nmod tests { fn t() { panic!("t") } }\npub fn f() { panic!("prod") }\n')"
expect_block "cfg(test) use; then prod panic"      "$(write "$P" $'#[cfg(test)]\nuse foo::{a, b};\npub fn f() { panic!("prod") }\n')"
expect_block "cfg(test) mod decl; then prod panic" "$(write "$P" $'#[cfg(test)]\nmod tests;\npub fn f() { panic!("prod") }\n')"
expect_block "allow(clippy::panic) is item-scoped" "$(write "$P" $'#[allow(clippy::panic)]\nfn a() { panic!("ok") }\nfn b() { panic!("prod") }\n')"
expect_block "tests mentioned only in a string"    "$(write "$P" $'const S: &str = "#[cfg(test)]";\npub fn f() { panic!("prod") }\n')"
expect_block "std::panic! and brace form"          "$(write "$P" 'pub fn f() { std::panic!{"x"} }')"
expect_block "URL string before panic"            "$(write "$P" 'pub fn f() { let u = "http://a"; panic!("p") }')"
expect_block "spaced invocation"                  "$(write "$P" 'pub fn f() { panic! ("x") }')"
expect_block "name ending in tests.rs"             "$(write "$SRC/contests.rs" 'pub fn f() { panic!("x") }')"
expect_block "test word in a file name only"       "$(write "$SRC/latest.rs" 'pub fn f() { panic!("x") }')"
expect_block "path with quote still blocks"        "$(write "$SRC/a\"b.rs" 'pub fn f() { panic!("x") }')"
expect_block "unwrap too: block wins, no JSON"     "$(write "$P" 'pub fn f() { a.unwrap(); panic!("x") }')"

echo "# malformed input (allowed silently: never invalid stdout, never a crash)"
expect_silent "empty stdin"           ''
expect_silent "not JSON"              'this is not json {'
expect_silent "JSON array"            '[1,2,3]'
expect_silent "JSON string"           '"panic!(\"x\")"'
expect_silent "tool_input is string"  '{"tool_name":"Write","tool_input":"panic!(\"x\")"}'
expect_silent "file_path is a number" '{"tool_name":"Write","tool_input":{"file_path":7,"content":"panic!(\"x\")"}}'
expect_silent "content is an object"  "$(jq -nc --arg p "$P" '{tool_name:"Write",tool_input:{file_path:$p,content:{a:"panic!(\"x\")"}}}')"
expect_silent "truncated JSON"        "$(write "$P" 'pub fn f() { panic!("x") }' | head -c 30)"

echo "# missing jq degrades to allow (documented in HOOKS.md)"
mkdir -p "$T/bin"
for tool in bash cat awk grep; do ln -s "$(command -v "$tool")" "$T/bin/$tool"; done
printf '%s' "$(write "$P" 'pub fn f() { panic!("x") }')" | PATH="$T/bin" "$T/bin/bash" "$HOOK" >"$T/out" 2>"$T/err"
RC=$?; OUT=$(cat "$T/out"); ERR=$(cat "$T/err")
if [[ $RC -eq 0 && -z "$OUT" ]]; then ok "no jq on PATH"; else bad "no jq on PATH" "expected silent exit 0"; fi

echo
echo "passed=$PASS failed=$FAIL"
[[ $FAIL -eq 0 ]]
