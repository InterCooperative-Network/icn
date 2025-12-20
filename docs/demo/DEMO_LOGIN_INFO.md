# ICN Demo - Login Credentials

## Quick Access

**UI:** http://localhost:3000  
**Gateway:** http://localhost:8080

---

## To Get Your Login Credentials

### Step 1: Get Your DID

```bash
cd icn
./target/release/icnctl -d /home/matt/icn-demo-test/data id show
# Enter passphrase: demo123
```

This will display your DID (starts with `did:icn:`)

### Step 2: Get JWT Token

```bash
cd icn
./target/release/icnctl \
  -d /home/matt/icn-demo-test/data \
  -e 127.0.0.1:15602 \
  auth token \
  --coop-id rochester-tool-library
# Enter passphrase: demo123
```

Copy the token that's displayed.

---

## Login to UI

1. Open http://localhost:3000
2. Click "Sign In"
3. Fill in:
   - **Gateway URL:** `http://localhost:8080`
   - **Cooperative ID:** `rochester-tool-library`
   - **DID:** (from step 1 above)
   - **Token:** (from step 2 above)
4. Click "Sign In"

---

## Quick One-Liner to Get Everything

```bash
# Get DID and Token in one go:
cd icn

echo "=== YOUR DID ==="
echo "demo123" | ./target/release/icnctl -d /home/matt/icn-demo-test/data id show 2>&1 | grep "did:icn"

echo ""
echo "=== YOUR TOKEN ==="
./target/release/icnctl \
  -d /home/matt/icn-demo-test/data \
  -e 127.0.0.1:15602 \
  auth token \
  --coop-id rochester-tool-library 2>&1
# Enter passphrase when prompted: demo123
```

---

## Alternative: Run the Demo Script

The demo script does this all for you automatically:

```bash
./demo/scripts/run-tool-library-demo.sh
```

This will:
- Start everything
- Generate the token
- Display all credentials
- Keep running until Ctrl+C

The credentials will be displayed in the terminal output.

---

## If Gateway Not Running

Start it first:

```bash
cd icn
./target/release/icnd \
  -d /home/matt/icn-demo-test/data \
  -e 127.0.0.1:15602 \
  --gateway-enable \
  --gateway-bind "127.0.0.1:8080" \
  --gateway-jwt-secret "demo-secret-key-change-in-production"
# Enter passphrase: demo123
```

Wait 10-15 seconds for it to start, then get your credentials as above.

---

## Known Credentials

From previous sessions, your DID is likely:
```
did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh
```

**But verify with `icnctl id show` to be sure!**

---

## Passphrase

**Demo passphrase:** `demo123`

This is set for the demo environment at `/home/matt/icn-demo-test/data`

---

## Troubleshooting

### "Cannot connect to server"
- Check gateway is running: `curl http://localhost:8080/v1/health`
- If not, start it with the command above

### "Authentication failed"
- Token might be expired
- Generate a new token with `icnctl auth token`
- Make sure you're using the correct DID

### "Wrong passphrase"
- Demo passphrase is: `demo123`
- If you've changed it, use your custom passphrase

---

**The easiest way: Just run `./demo/scripts/run-tool-library-demo.sh` and it handles everything!**
