# ICN Release Process

**Version**: 1.0  
**Last Updated**: 2025-12-16  
**Audience**: Core maintainers

---

## Release Types

ICN follows [Semantic Versioning 2.0.0](https://semver.org/):

- **Major (x.0.0)**: Breaking changes, incompatible API changes
- **Minor (0.x.0)**: New features, backward compatible
- **Patch (0.0.x)**: Bug fixes, backward compatible

### Pre-release Tags
- **Alpha (x.y.z-alpha.n)**: Early development, unstable
- **Beta (x.y.z-beta.n)**: Feature complete, testing
- **RC (x.y.z-rc.n)**: Release candidate, final testing

---

## Release Checklist

### 1. Pre-Release (1 week before)

- [ ] **Create release branch**: `git checkout -b release/v0.x.0`
- [ ] **Run full test suite**: `cargo test --workspace --release`
- [ ] **Run benchmarks**: `cargo bench --workspace`
- [ ] **Run security audit**: `cargo audit`
- [ ] **Update dependencies**: `cargo update` (review changes carefully)
- [ ] **Update CHANGELOG.md**: Document all changes since last release
- [ ] **Update version numbers**:
  ```bash
  # Update Cargo.toml workspace version
  sed -i 's/version = "0.1.0"/version = "0.2.0"/' icn/Cargo.toml
  ```
- [ ] **Update documentation**: Ensure all docs reflect new version
- [ ] **Test deployment**: Deploy to staging environment
- [ ] **Notify pilot communities**: Announce upcoming release

### 2. Release Candidate (3 days before)

- [ ] **Tag RC**: `git tag v0.2.0-rc.1 && git push origin v0.2.0-rc.1`
- [ ] **Build RC binaries**:
  ```bash
  cargo build --release
  tar -czf icn-v0.2.0-rc.1-linux-x86_64.tar.gz -C target/release icnd icnctl
  sha256sum icn-v0.2.0-rc.1-linux-x86_64.tar.gz > icn-v0.2.0-rc.1-linux-x86_64.tar.gz.sha256
  ```
- [ ] **Test RC in staging**: Run integration tests, load tests
- [ ] **Gather feedback**: Pilot communities test RC for 48 hours
- [ ] **Fix critical issues**: If found, create RC.2, RC.3, etc.

### 3. Release Day

- [ ] **Final checks**: All tests pass, no critical issues
- [ ] **Merge release branch**: 
  ```bash
  git checkout main
  git merge --no-ff release/v0.2.0
  ```
- [ ] **Tag release**: `git tag v0.2.0 && git push origin v0.2.0`
- [ ] **Build release binaries**:
  ```bash
  cargo build --release
  # Linux
  tar -czf icn-v0.2.0-linux-x86_64.tar.gz -C target/release icnd icnctl icn-console
  sha256sum icn-v0.2.0-linux-x86_64.tar.gz > icn-v0.2.0-linux-x86_64.tar.gz.sha256
  
  # macOS (if cross-compiling or on macOS)
  # tar -czf icn-v0.2.0-macos-x86_64.tar.gz -C target/release icnd icnctl icn-console
  ```
- [ ] **Create GitHub Release**:
  - Title: `v0.2.0 - Release Name`
  - Body: Copy from CHANGELOG.md
  - Attach: Binaries + checksums
  - Mark as "Latest Release"
- [ ] **Publish SDKs**:
  ```bash
  cd sdk/typescript
  npm version 0.2.0
  npm publish
  
  cd ../react-native
  npm version 0.2.0
  npm publish
  ```
- [ ] **Update documentation site**: Deploy updated docs
- [ ] **Announce release**:
  - GitHub Discussions
  - Discord/Matrix
  - Mailing list
  - Social media

### 4. Post-Release (1 day after)

- [ ] **Monitor for issues**: Watch GitHub issues, Discord, logs
- [ ] **Update pilot deployments**: Coordinate upgrade with pilots
- [ ] **Archive release branch**: Delete release/v0.2.0 branch
- [ ] **Plan next release**: Create milestone for v0.3.0

---

## CHANGELOG Format

Follow [Keep a Changelog](https://keepachangelog.com/):

```markdown
# Changelog

## [Unreleased]

### Added
- Feature X for Y use case

### Changed
- Improved Z performance by 20%

### Deprecated
- Legacy API endpoint /old/path

### Removed
- Support for deprecated feature W

### Fixed
- Bug causing crash on startup (#123)

### Security
- Patched CVE-2025-1234 in dependency X

## [0.2.0] - 2025-12-16

### Added
- Multi-device identity support (Phase 11)
- Economic safety rails (Phase 12)
- Gateway REST API (Phase 14)

...
```

---

## Version Numbering Strategy

### Current Phase: 0.x.y (Pre-1.0)

**Major version stays at 0** until pilot-proven and API stable.

**Minor version increments** for:
- New substrate features (Phase completions)
- New SDK capabilities
- Significant architectural changes

**Patch version increments** for:
- Bug fixes
- Performance improvements
- Documentation updates
- Security patches

### Target: 1.0.0 Release Criteria

To release 1.0.0, we need:
- [ ] 3+ successful pilot deployments (6+ months each)
- [ ] Complete Phase 1-20 (substrate complete)
- [ ] API stability guarantee (no breaking changes for 1 year)
- [ ] Security audit completed with no critical findings
- [ ] Production deployment guide validated
- [ ] 80%+ test coverage
- [ ] Documentation complete for all features
- [ ] SDK stability (TypeScript, React Native)
- [ ] Performance benchmarks meet SLOs
- [ ] Disaster recovery tested and verified

---

## Breaking Change Policy

### Pre-1.0 (Current)

Breaking changes are **allowed** but should be:
- Documented in CHANGELOG with `[BREAKING]` tag
- Announced 1 release in advance (deprecation warning)
- Accompanied by migration guide

Example:
```markdown
## [0.3.0] - 2025-01-15

### Changed
- [BREAKING] Gateway API authentication now requires `Bearer` token prefix (#456)
  - Migration: Update clients to send `Authorization: Bearer <token>`
  - Old format deprecated in v0.2.0, removed in v0.3.0
```

### Post-1.0

Breaking changes **only in major versions**:
- v1.x.y → v2.0.0
- Requires 6+ months deprecation period
- Must provide compatibility layer or migration tool
- Documented upgrade path required

---

## Hotfix Process

For critical bugs in production:

1. **Assess severity**:
   - Security vulnerability: Immediate hotfix
   - Data loss risk: Immediate hotfix
   - Service disruption: Immediate hotfix
   - Other: Can wait for next release

2. **Create hotfix branch**:
   ```bash
   git checkout v0.2.0
   git checkout -b hotfix/v0.2.1
   ```

3. **Fix and test**:
   ```bash
   # Make minimal changes to fix issue
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

4. **Release hotfix**:
   ```bash
   # Update version to 0.2.1
   git commit -am "Hotfix: Fix critical issue #789"
   git tag v0.2.1
   git push origin v0.2.1
   
   # Merge back to main
   git checkout main
   git merge hotfix/v0.2.1
   git push origin main
   ```

5. **Notify users**: Urgent release announcement

---

## Compatibility Matrix

Document version compatibility in releases:

| ICN Version | TypeScript SDK | React Native SDK | Min Rust | Min Protocol |
|-------------|---------------|------------------|----------|--------------|
| 0.2.0 | 0.2.x | 0.2.x | 1.75.0 | v1 |
| 0.1.0 | 0.1.x | 0.1.x | 1.75.0 | v1 |

---

## Release Automation

### GitHub Actions Workflow (Future)

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build-release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      
      - name: Build release binaries
        run: |
          cargo build --release
          tar -czf icn-${{ github.ref_name }}-linux-x86_64.tar.gz \
            -C target/release icnd icnctl icn-console
          sha256sum icn-${{ github.ref_name }}-linux-x86_64.tar.gz > \
            icn-${{ github.ref_name }}-linux-x86_64.tar.gz.sha256
      
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            icn-${{ github.ref_name }}-linux-x86_64.tar.gz
            icn-${{ github.ref_name }}-linux-x86_64.tar.gz.sha256
          body_path: RELEASE_NOTES.md
```

---

## Communication Templates

### Release Announcement (Discord/GitHub)

```markdown
🎉 **ICN v0.2.0 Released!**

We're excited to announce ICN v0.2.0, featuring:

✨ **New Features**
- Multi-device identity support
- Economic safety rails
- Gateway REST API

🐛 **Bug Fixes**
- Fixed gossip convergence issue (#123)
- Improved trust computation performance

📦 **Downloads**
- [Linux x86_64](https://github.com/icn/icn/releases/download/v0.2.0/icn-v0.2.0-linux-x86_64.tar.gz)
- [Changelog](https://github.com/icn/icn/blob/main/CHANGELOG.md)

📚 **Upgrade Guide**: See [version-upgrades.md](../migration-guides/version-upgrades.md)

Questions? Ask in #support
```

### Security Hotfix Announcement

```markdown
🚨 **Security Hotfix: ICN v0.2.1**

A security vulnerability was discovered that could allow [brief description].

**Severity**: High
**CVE**: CVE-2025-XXXX (if assigned)
**Affected Versions**: v0.2.0 and earlier
**Fixed In**: v0.2.1

**Action Required**: Upgrade immediately
- Download: [v0.2.1](https://github.com/icn/icn/releases/tag/v0.2.1)
- Upgrade guide: [version-upgrades.md](../migration-guides/version-upgrades.md)

**Details**: [Security Advisory](https://github.com/icn/icn/security/advisories/GHSA-XXXX)

For questions, contact security@icn.coop (PGP: 0xABCD1234)
```

---

## Deprecation Policy

1. **Announce deprecation** in release notes and documentation
2. **Add runtime warnings** to deprecated features
3. **Keep for 2 minor versions** (minimum)
4. **Remove in next major version** (post-1.0)

Example:
```rust
#[deprecated(since = "0.2.0", note = "Use new_function() instead")]
pub fn old_function() {
    // ...
}
```

---

## Release Cadence

**Target Schedule**:
- **Major releases**: Annually (1.0, 2.0, ...)
- **Minor releases**: Quarterly (0.1, 0.2, 0.3, 0.4)
- **Patch releases**: As needed (bug fixes, security)
- **Pre-releases**: 2 weeks before each release

**Release Windows**:
- Prefer mid-week (Tuesday/Wednesday)
- Avoid holidays
- Coordinate with pilot communities

---

## Roll Back Procedure

If a release has critical issues:

1. **Assess impact**: Can we hotfix or must we rollback?
2. **Communicate**: Alert all users immediately
3. **Yank release** (if published to package managers):
   ```bash
   npm unpublish icn-sdk@0.2.0
   cargo yank --vers 0.2.0 icn-core
   ```
4. **Remove GitHub release**: Unmark as "Latest Release"
5. **Document issue**: Post-mortem analysis
6. **Prepare fix**: Either hotfix 0.2.1 or revert to 0.1.x

---

## Tooling

**Required**:
- Git
- Rust toolchain
- cargo (cargo-audit, cargo-outdated)
- GitHub CLI (gh)
- sha256sum

**Optional**:
- cargo-release (automates version bumping)
- conventional-changelog (generates CHANGELOG from commits)

---

## Questions & Support

- **Release issues**: Open issue with `release` label
- **Security concerns**: security@icn.coop
- **General questions**: #releases on Discord

---

**Document Maintainer**: Core Team  
**Review Cadence**: Before each major/minor release  
**Last Review**: 2025-12-16
