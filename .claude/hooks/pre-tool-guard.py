import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
tool_input = data.get('tool_input', {})
path = tool_input.get('file_path', '') or tool_input.get('path', '')
protected = {
    'icn-identity':    'DID/key management -- public surface changes require ADR + backward-compat check',
    'icn-ledger':      'double-entry mutual credit -- invariant violations cause unrecoverable state',
    'icn-governance':  'voting/proposal logic -- protocol changes need quorum analysis',
    'icn-ccl':         'cooperative contract language -- interpreter changes affect all deployed contracts',
    'icn-trust':       'trust graph -- scoring changes affect access control across the network',
}
for crate, warning in protected.items():
    if crate in path:
        print(f'[icn-dev GUARD] Protected crate: {crate}', flush=True)
        print(f'  Warning: {warning}', flush=True)
        print(f'  Checklist: [ ] No public API change [ ] Tests added [ ] ADR filed if protocol shape changed [ ] No payment/currency/token terminology', flush=True)
        break
