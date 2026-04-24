# ci-runner configuration

Configuration artifacts for the ICN self-hosted GitHub Actions runner
(`ci-runner`, VM at `10.8.30.46`). The runner itself is installed under
`~/actions-runner` on the VM; this directory holds the repo-backed pieces that
operators should apply to the host.

## Files

| File | Purpose |
|------|---------|
| `atlas-sccache-setup.sh` | Mount the shared Atlas-backed sccache and point the runner env at it. See issue #1597. |

## Atlas-backed sccache

The runner historically used a local 10G cache at `~/.cache/sccache`. On a
77G root filesystem that fills to 80%+ under routine Rust builds, which in turn
starves builds. `icn-dev` (`10.8.30.45`) has long used a shared NFS cache at
`/mnt/icn-sccache` backed by Atlas (`10.8.10.25`, ~892G pool). This directory
captures the same setup for `ci-runner`.

### Apply

Run on the ci-runner host **in a quiet window** (no active GitHub Actions
jobs — the final step restarts the runner service):

```bash
# From a workstation:
scp deploy/ci-runner/atlas-sccache-setup.sh icn-dev-cursor:/tmp/
ssh icn-dev-cursor "scp /tmp/atlas-sccache-setup.sh ubuntu@10.8.30.46:/tmp/"
ssh icn-dev-cursor "ssh ubuntu@10.8.30.46 'sudo bash /tmp/atlas-sccache-setup.sh'"
```

The script is idempotent:
- Only installs `nfs-common` if missing.
- Only appends to `/etc/fstab` if no entry for `/mnt/icn-sccache` exists.
- Only mounts if the path isn't already a mountpoint.
- Only adds the sccache block to `~ubuntu/.bashrc` if its tagged marker is absent.
- Rewrites `RUSTC_WRAPPER`, `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE` in
  `~ubuntu/actions-runner/.env`, preserving other lines.
- Snapshots `/etc/fstab`, mount table, df, and the runner env to
  `/tmp/ci-runner-sccache-snapshot-<timestamp>/` before mutating.

### Skip the runner restart

To apply the mount and env changes without restarting the runner service
(useful while a job is active — the env change won't take effect until the
next restart):

```bash
sudo SKIP_RESTART=1 bash /tmp/atlas-sccache-setup.sh
# later, in a quiet window:
sudo systemctl restart actions.runner.InterCooperative-Network-icn.ci-runner.service
```

If `Runner.Worker` is active and `SKIP_RESTART` is not set, the script
refuses to restart and exits non-zero — rerun later or use `SKIP_RESTART=1`.

### Verify

```bash
ssh icn-dev-cursor "ssh ubuntu@10.8.30.46 '
  df -hT /mnt/icn-sccache
  findmnt /mnt/icn-sccache
  grep SCCACHE ~/actions-runner/.env
  systemctl status actions.runner.InterCooperative-Network-icn.ci-runner.service --no-pager --lines=0
'"
```

Expected:
- `/mnt/icn-sccache` mounted on `10.8.10.25:/mnt/ssd_pool/icn-vols/sccache-cache`
  (NFS v4.2, `hard,noatime,nofail,_netdev`).
- `~/actions-runner/.env` contains `SCCACHE_DIR=/mnt/icn-sccache` and
  `SCCACHE_CACHE_SIZE=100G`.
- Runner service is `active (running)`.

### Rollback

The script does not reclaim the local `~/.cache/sccache` directory. To fully
revert:

```bash
sudo umount /mnt/icn-sccache
sudo sed -i '\|# ICN shared sccache (issue #1597)|,+1d' /etc/fstab
# restore ~/actions-runner/.env and ~/.bashrc from the snapshot directory
sudo systemctl restart actions.runner.InterCooperative-Network-icn.ci-runner.service
```
