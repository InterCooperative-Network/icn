import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
path = data.get('tool_input', {}).get('file_path', '') or ''
if path.endswith('.rs'):
    short = path.split('icn/')[-1] if 'icn/' in path else path
    print(f'[icn-dev] .rs modified: {short} -- suggest: cargo check --workspace')
