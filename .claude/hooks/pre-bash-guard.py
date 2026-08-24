import sys, json
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
cmd = data.get('tool_input', {}).get('command', '')
if 'git checkout main' in cmd or 'git push origin main' in cmd:
    print('[icn-dev GUARD] Direct main branch op -- ICN uses feature branches. Confirm intentional.')
