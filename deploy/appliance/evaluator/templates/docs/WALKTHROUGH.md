# Browser Walkthrough

## Goal

See what exists today in the browser, running against the local VM.

## Setup Path

```text
extract zip
   |
install host tools
   |
run setup-and-run
   |
verify checksums automatically
   |
VM starts automatically
   |
open browser URL printed by launcher
```

## Browser Path

```text
member shell
   |
Start local demo
   |
standing/action card
   |
complete
   |
receipt
```

## Step 1: Start The VM

From the package root:

```bash
./setup-and-run.sh
```

On Debian/Ubuntu, this installs missing host tools, verifies the package, and
starts the VM. Wait until the terminal says:

```text
ICN demo VM is ready.
```

## Step 2: Open The Member Shell

Open:

```text
http://localhost:18090/member-shell/?mode=live&demo=launcher&gw=18080&session=18091
```

## Step 3: Start The Demo

Click:

```text
Start local demo
```

## Step 4: Complete The Action

Follow the action card shown in the page.

The intended click-through is:

```text
standing -> action card -> complete -> receipt
```

## Step 5: Confirm The Result

The receipt is the visible proof that the local browser flow completed.

## What To Notice

- The browser UI is served from the VM.
- The gateway health check is from the VM.
- The demo is local and self-contained after download.
- The original image remains unchanged because the launcher uses a throwaway
  overlay.

## Stop

Press `Ctrl-C` in the launcher terminal.
