# Frequently Asked Questions (FAQ)

## General Questions

### What is ICN?

ICN (Intercooperative Network) is a **peer-to-peer substrate for cooperatives**. It provides the infrastructure layer for cooperative coordination: identity, trust, mutual credit ledger, governance, and distributed computation.

Think of it as "Git for economic coordination" - it handles the decentralized substrate so cooperatives can focus on their mission.

### Is ICN a blockchain?

**No.** ICN is not a blockchain. Key differences:

| ICN | Blockchain |
|-----|------------|
| P2P gossip (no global consensus) | Global consensus (PoW/PoS) |
| Mutual credit (zero-sum) | Cryptocurrency (speculative) |
| Trust-gated access | Permissionless |
| Designed for cooperatives (10-1000 members) | Designed for global scale |
| Deterministic contracts (CCL) | Turing-complete (Solidity/etc) |

ICN uses **gossip** for eventual consistency, not global consensus. This is much more efficient for cooperative-scale coordination.

### Who is ICN for?

**Primary audience:**
- Housing cooperatives
- Worker cooperatives
- Timebanks
- Community land trusts
- Food co-ops
- Tool libraries
- Mutual aid networks

**Not for:**
- Corporations seeking blockchain hype
- Cryptocurrency speculation
- Global-scale financial infrastructure
- Adversarial environments (use blockchain instead)

### How is this different from [other project]?

**vs. Holochain:**
- ICN: Rust-based, actor runtime, CCL contracts
- Holochain: Holo/HDK, agent-centric, Wasm zomes
- Similarity: Both use gossip, both avoid global consensus

**vs. Secure Scuttlebutt (SSB):**
- ICN: Mutual credit ledger, governance, trust graph
- SSB: Social messaging, append-only logs
- Similarity: Both are P2P, both use gossip

**vs. Ethereum/Smart Contract Platforms:**
- ICN: Trust-first, cooperative-scale, deterministic
- Ethereum: Trustless, global-scale, Turing-complete
- Different: ICN is not a blockchain, no mining, no gas fees

### Is ICN production-ready?

**Current status (2025-11-21):**
- ✅ **Substrate**: Production-ready (Phase 15 complete, 423 tests passing)
- ✅ **Security**: Three-layer architecture (transport, message, application)
- ✅ **Economic Safety**: Dynamic credit limits, dispute resolution, new member throttling
- ✅ **Operations**: Backup/restore, monitoring, graceful restart
- 🚧 **Pilot Deployment**: Selecting first pilot community (Track C1)
- ❌ **General Availability**: Not yet (need pilot learnings first)

**Recommendation**: Wait for pilot results before deploying in production. You can experiment now, but expect rough edges.

---

## Technical Questions

### What language is ICN written in?

**Rust** (100%). Chosen for:
- Memory safety without garbage collection
- Excellent async runtime (Tokio)
- Strong type system
- Zero-cost abstractions
- Great cryptography libraries

### What are the system requirements?

**Minimum:**
- CPU: 1 core (2+ recommended)
- RAM: 512 MB (1 GB+ recommended)
- Disk: 1 GB (10 GB+ for active node)
- OS: Linux, macOS, or Windows (WSL2)
- Network: 1 Mbps+ (5 Mbps+ for active node)

**Recommended for pilot (10-50 members):**
- CPU: 2 cores
- RAM: 2 GB
- Disk: 20 GB SSD
- OS: Linux (Ubuntu 22.04 LTS or Debian 12)
- Network: 10 Mbps+ with static IP

### Does ICN work on Raspberry Pi?

**Yes!** ICN runs on ARM64:
- Raspberry Pi 4 (4 GB+ recommended)
- Raspberry Pi 5
- Other ARM64 SBCs

Build from source:
```bash
cargo build --release --target aarch64-unknown-linux-gnu
```

### How do I connect nodes over the internet (not just LAN)?

ICN uses **mDNS for local discovery** and **QUIC for connections**. For internet connections:

1. **Manual peering** (easiest):
   ```bash
   icnctl network add-peer <public-ip>:5600 <did>
   ```

2. **NAT traversal** (Phases 1-3 complete):
   - STUN for public IP discovery
   - Hole punching for NAT traversal
   - Configurable STUN servers

3. **TURN relay** (Phase 4, not yet implemented):
   - For strict corporate firewalls
   - Self-hosted TURN server

4. **VPN/Tailscale** (works today):
   - Set up Tailscale/Wireguard
   - Use Tailscale IPs for peering

See [nat-traversal-design.md](nat-traversal-design.md) for details.

### Can I run ICN in Docker?

**Yes!** Example Dockerfile:
```dockerfile
FROM rust:1.75 as builder
WORKDIR /icn
COPY . .
RUN cd icn && cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /icn/icn/target/release/icnd /usr/local/bin/
COPY --from=builder /icn/icn/target/release/icnctl /usr/local/bin/
EXPOSE 5600/tcp 5600/udp 8080/tcp 9100/tcp
CMD ["icnd"]
```

**Data persistence:**
Mount `~/.icn` as a volume to persist identity and state.

### What ports does ICN use?

- **5600/TCP+UDP**: QUIC networking (P2P connections)
- **8080/TCP**: Gateway API (REST + WebSocket)
- **9100/TCP**: Prometheus metrics
- **5353/UDP**: mDNS discovery (LAN only)

**Firewall configuration:**
```bash
# Allow ICN P2P
sudo ufw allow 5600/tcp
sudo ufw allow 5600/udp

# Allow Gateway API (if hosting apps)
sudo ufw allow 8080/tcp

# Allow metrics (if using Prometheus)
sudo ufw allow 9100/tcp
```

---

## Security & Privacy Questions

### How is my identity secured?

- **Private key**: Stored in Age-encrypted keystore (`~/.icn/keystore.age`)
- **Passphrase**: You choose during `icnctl id init`
- **Encryption**: Age encryption (ChaCha20-Poly1305)
- **Backup**: Use `icnctl backup` to create encrypted backups

**Best practices:**
- Use a strong passphrase (12+ characters, mix of types)
- Backup your keystore off-site
- Consider hardware keys (YubiKey support planned)

### What if I lose my device?

**If you have a backup:**
```bash
icnctl restore ~/backup.tar.gz.age
```

**If you don't have a backup:**
- Your identity is lost (can't be recovered)
- Create a new identity with `icnctl id init`
- Request trust re-attestation from cooperative members

**Multi-device identity (recommended):**
Use multiple devices so losing one doesn't lose your identity:
```bash
icnctl device add --name "My Phone"
```

See [multi-device-identity-design.md](multi-device-identity-design.md).

### Is communication encrypted?

**Yes**, three layers:
1. **Transport**: QUIC with TLS 1.3 (all network traffic)
2. **Message**: SignedEnvelope with Ed25519 signatures
3. **Application**: EncryptedEnvelope with X25519-ChaCha20-Poly1305 (optional)

All P2P communication is encrypted by default.

### Can cooperatives spy on each other?

**No.** ICN is **multi-tenant by design**:
- Each cooperative has its own ledger, trust graph, and governance
- Gossip topics are isolated by access control (Public, Private, TrustGated)
- Private topics are only visible to authorized members
- No cross-cooperative data leakage

### What about GDPR compliance?

ICN provides tools for GDPR compliance:
- **Right to access**: Export data with `icnctl backup`
- **Right to erasure**: Delete keystore and request pruning from peers
- **Data minimization**: Only stores necessary data (no PII by default)
- **Encryption**: All data encrypted at rest and in transit

**Cooperative responsibility:**
- Establish data retention policies
- Document processing activities
- Appoint data protection officer (if required)
- Provide privacy policy to members

See [legal-considerations.md](legal-considerations.md) for details.

---

## Usage Questions

### How do I set credit limits?

Credit limits are set by the **cooperative's credit policy**. Options:

1. **Use defaults** (from Phase 12 economic modeling):
   - Initial: -20 credits
   - Maximum: -500 credits
   - Bonus: +10 per 50 cleared transactions
   - Trust multiplier: 2x

2. **Override per-member** (admin):
   ```bash
   # Not yet exposed in CLI, use RPC or CCL contract
   ```

3. **Configure policy** (via CCL contract):
   ```ccl
   // Define credit policy in cooperative contract
   // See docs/economic-safety.md
   ```

See [economic-safety.md](economic-safety.md) for details.

### Can I use multiple currencies?

**Yes!** The ledger supports multiple currencies:
- Hours (timebanking)
- USD (or local currency)
- Credits (generic)
- kWh (energy)
- Custom units

Each currency has separate balances and credit policies.

```bash
icnctl ledger pay <did> 2 hours "Gardening help"
icnctl ledger pay <did> 10 USD "Grocery share"
```

### How do I resolve a disputed transaction?

1. **File a dispute**:
   ```bash
   icnctl ledger dispute <entry-hash> "Reason for dispute"
   ```

2. **Cooperative mediates**:
   - Assign a mediator (via governance proposal)
   - Gather evidence
   - Decide outcome

3. **Resolve**:
   ```bash
   icnctl ledger resolve <entry-hash> <outcome> "Reasoning"
   ```

Outcomes: `Uphold` (keep entry), `Reverse` (cancel entry), `Adjust` (modify amount)

See [economic-safety.md](economic-safety.md) for details.

### Can I delete my transaction history?

**No.** The ledger is **immutable by design** to maintain trust and accountability.

However:
- You can **dispute** incorrect entries
- You can **write off** bad debts (with governance approval)
- Old entries can be **archived** (removed from active state but kept in backups)

### How does governance work?

ICN provides **governance primitives**, not a fixed system:

1. **Create a governance domain**:
   ```bash
   icnctl gov domain create --domain-id my-coop --name "My Cooperative"
   ```

2. **Create a proposal**:
   ```bash
   icnctl gov proposal create --domain-id my-coop \
     --title "Approve new supplier" --kind text
   ```

3. **Open for voting**:
   ```bash
   icnctl gov proposal open --proposal-id <id>
   ```

4. **Vote**:
   ```bash
   icnctl gov vote cast --proposal-id <id> --choice for
   ```

5. **Close and tally**:
   ```bash
   icnctl gov proposal close --proposal-id <id>
   ```

See [governance-primitives.md](governance-primitives.md) for details.

---

## Troubleshooting

### "No peers connected" - What do I do?

**Check network connectivity:**
```bash
icnctl network status
icnctl network peers
```

**Common causes:**
1. **Firewall blocking port 5600**: Open firewall
2. **No peers on LAN**: Manually add peer with `icnctl network add-peer`
3. **mDNS not working**: Use manual peering
4. **NAT issues**: Configure STUN or use VPN

**Solution:**
```bash
# Manual peering (most reliable)
icnctl network add-peer <ip>:5600 <did>
```

### Tests are failing - What's wrong?

**Common causes:**
1. **Port already in use**: Another ICN instance running
2. **Timing issues**: Increase timeouts in flaky tests
3. **Disk full**: Clean up `/tmp` and `~/.icn`
4. **Concurrent tests**: Run with `--test-threads=1`

**Debug:**
```bash
# Run with logs
RUST_LOG=debug cargo test <test_name> -- --nocapture

# Run single-threaded
cargo test -- --test-threads=1

# Clean and rebuild
cargo clean && cargo build && cargo test
```

### Daemon won't start - What do I check?

**Check logs:**
```bash
# Systemd
journalctl -u icnd -f

# Manual run
RUST_LOG=debug icnd
```

**Common issues:**
1. **Port 5600 in use**: Another process using the port
2. **Keystore locked**: Wrong passphrase or corrupted keystore
3. **Permissions**: Can't write to `~/.icn/`
4. **Missing dependencies**: Reinstall ICN

### How do I reset everything and start fresh?

**CAUTION**: This deletes your identity and all data!

```bash
# Stop daemon
sudo systemctl stop icnd

# Backup (optional)
icnctl backup ~/icn-backup-before-reset.tar.gz.age

# Delete data
rm -rf ~/.icn/

# Start fresh
icnctl id init
```

---

## Roadmap & Future

### What's next for ICN?

See [ROADMAP.md](../ROADMAP.md) for detailed plans. Summary:

**Immediate (2025 Q1):**
- Track C1: Select pilot community
- Deploy pilot with real cooperative
- Weekly learning loop and feedback

**Short-term (2025 Q2-Q3):**
- Incorporate pilot learnings
- TypeScript SDK and reference app
- Additional governance patterns
- Federation (if multiple pilots)

**Long-term (2025 Q4+):**
- Multi-cooperative federation
- Advanced privacy features
- Mobile apps (if web isn't sufficient)
- Scale pilots to 10+ communities

### Can I use ICN for [X]?

**Good fit:**
- Timebanks
- Housing cooperatives
- Worker cooperatives
- Tool libraries
- Community currencies
- Mutual aid networks

**Maybe:**
- Social networks (use SSB instead?)
- Supply chain tracking (depends on scale)
- Voting systems (governance primitives help)

**Not a good fit:**
- Global financial infrastructure (use blockchain)
- Adversarial environments (use blockchain)
- Real-time gaming (latency issues)
- Centralized applications (use traditional DB)

**Ask yourself:**
- Is this a cooperative/community organization?
- Do you need decentralized coordination?
- Is the scale 10-1000 members?
- Is trust a reasonable assumption?

If yes to all four, ICN is likely a good fit!

---

## Getting Help

**Documentation:**
- [Getting Started Guide](GETTING_STARTED.md)
- [Architecture Overview](ARCHITECTURE.md)
- [Operations Guide](operations-guide.md)
- [All docs](.) in `docs/` directory

**Community:**
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Roadmap: [ROADMAP.md](../ROADMAP.md)
- Contributing: [CONTRIBUTING.md](../CONTRIBUTING.md)

---

*Have a question not answered here? [Open an issue](https://github.com/InterCooperative-Network/icn/issues/new) and we'll add it to the FAQ!*
