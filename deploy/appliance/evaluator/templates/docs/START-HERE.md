# Start Here: ICN Common Sense Bootable Vertical Slice

## What You Have

This package is a self-contained bootable vertical slice.

It contains a VM image and scripts that let you boot ICN locally and see the
current browser-facing member shell working today.

## The Whole Flow

```text
1. Extract zip
      |
      v
2. Install host VM tools
      |
      v
3. Run setup-and-run
      |
      v
4. Let it install/check tools
      |
      v
5. It verifies checksums
      |
      v
6. It starts the VM
      |
      v
7. Open the browser URL
      |
      v
8. Click through to receipt
```

## What You Will See

```text
+----------------------+        +--------------------------+
| Your machine         |        | Local browser            |
|                      |        |                          |
|  QEMU starts VM      | -----> |  Member Shell opens      |
|  icnd starts in VM   |        |  Start local demo button |
|  Gateway becomes OK  |        |  Action card             |
|  Shell is served     |        |  Receipt                 |
+----------------------+        +--------------------------+
```

## Step 1: Extract

Unzip the package. You should get:

```text
@PKG_NAME@/
```

Enter it:

```bash
cd @PKG_NAME@
```

## Step 2: Run The One-Command Setup

On Debian or Ubuntu, run:

```bash
./setup-and-run.sh
```

The script:

- installs missing host tools with `apt-get`,
- asks for `sudo` only if needed,
- verifies checksums,
- starts the local demo VM.

You may be asked for your system password so the script can install QEMU and
related VM tools.

## Step 3: Watch For Verification

You want the checksum lines to end with:

```text
OK
```

## Step 4: Wait For The VM

Leave the terminal open. The launcher waits until everything inside the VM is
ready.

## Step 5: Open the Browser

When the launcher says the demo VM is ready, open:

```text
http://localhost:18090/member-shell/?mode=live&demo=launcher&gw=18080&session=18091
```

## Step 6: Click Through

In the browser:

```text
Start local demo -> action card -> complete -> receipt
```

## Step 7: Stop

Go back to the terminal and press:

```text
Ctrl-C
```

## Manual Path

If you do not want the setup script to install packages, run:

```bash
./setup-and-run.sh --no-install
```

Or do the steps manually:

```bash
sudo apt-get update
sudo apt-get install -y qemu-system-x86 qemu-utils cloud-image-utils openssh-client curl coreutils
./scripts/verify.sh
./scripts/run-demo.sh
```

## What Exists Today In This Slice

This package shows:

- a bootable ICN demo image,
- `icnd` starting inside the VM,
- gateway health inside the VM,
- the current member shell served from the VM,
- a local browser demo flow,
- a receipt at the end of the flow.

## What This Does Not Claim

This does not claim:

- production readiness,
- a signed release,
- a live public ICN node,
- completed cooperative storage hosting,
- completed distributed compute hosting.

It is the working vertical slice that exists today.
