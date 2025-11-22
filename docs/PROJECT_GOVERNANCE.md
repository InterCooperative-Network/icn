# ICN Project Governance

This document describes how the ICN project is governed, how decisions are made, and how the community can participate.

## Mission

**ICN's mission** is to provide robust, secure, and accessible infrastructure for cooperative coordination. We are building substrate for the cooperative economy - not a product, but a foundation.

**Core Values:**
- **Cooperative First**: Design decisions prioritize cooperative use cases
- **Security & Privacy**: No compromise on member data protection
- **Simplicity**: Prefer simple, understandable solutions over clever complexity
- **Pilot-Driven**: Real-world feedback drives roadmap priorities
- **Open Development**: Transparent decision-making and open source code

---

## Project Structure

### Roles

#### Maintainers

**Responsibilities:**
- Review and merge pull requests
- Triage issues
- Make architectural decisions
- Release management
- Community stewardship

**Current Maintainers:**
- Matt (lead developer)

**Becoming a Maintainer:**
- Demonstrate sustained, high-quality contributions (6+ months)
- Deep understanding of ICN architecture
- Commitment to project values
- Nominated by existing maintainer(s)
- Consensus approval from maintainer team

#### Contributors

**Anyone who contributes** to ICN (code, docs, design, testing, feedback):
- Submit pull requests
- Report bugs and suggest features
- Participate in discussions
- Help other users

**No formal process**: Just start contributing! See [CONTRIBUTING.md](../CONTRIBUTING.md).

#### Pilot Community Partners

**Cooperatives piloting ICN**:
- Provide real-world feedback
- Shape roadmap priorities
- Test new features
- Document use cases

**Relationship**: Partnership, not hierarchy. Pilots guide development but don't control it.

---

## Decision-Making Process

### Types of Decisions

#### 1. Routine Decisions (Fast Track)

**Examples:**
- Bug fixes
- Documentation updates
- Dependency updates (security patches)
- Clippy/fmt fixes

**Process:**
1. Contributor opens PR
2. Maintainer reviews
3. Merge if quality standards met
4. No discussion needed for obvious fixes

**Timeline**: 1-3 days

#### 2. Feature Additions (Standard Track)

**Examples:**
- New CLI commands
- API endpoint additions
- Performance optimizations
- New integrations

**Process:**
1. Open GitHub issue to discuss
2. Maintainer provides feedback
3. If approved, contributor implements
4. PR review and merge
5. Update CHANGELOG and docs

**Timeline**: 1-2 weeks

#### 3. Architectural Changes (RFC Track)

**Examples:**
- New protocol features (e.g., federation)
- Breaking API changes
- Major refactors
- New subsystems

**Process:**
1. **RFC (Request for Comments)**:
   - Create issue with "RFC:" prefix
   - Describe problem, proposed solution, alternatives
   - Solicit community feedback
   - Iterate on design

2. **Evaluation**:
   - Maintainers assess technical merit
   - Consider alignment with mission
   - Evaluate complexity vs. benefit
   - Check pilot community needs

3. **Decision**:
   - Consensus among maintainers (aim for unanimous)
   - If disagreement, lead maintainer decides
   - Decision documented in RFC issue

4. **Implementation**:
   - Break into milestones
   - Track in project board
   - Regular progress updates

**Timeline**: 2-8 weeks (discussion) + implementation time

**Example RFCs:**
- Multi-device identity (Phase 11)
- Economic safety rails (Phase 12)
- Gateway API (Phase 14)

#### 4. Roadmap Priorities (Community-Driven)

**Examples:**
- Which features to build next
- Pilot deployment timelines
- Resource allocation

**Process:**
1. **Input Gathering**:
   - Pilot community feedback
   - GitHub issue discussions
   - Contributor proposals
   - User surveys

2. **Prioritization**:
   - Maintainers synthesize feedback
   - Draft roadmap update
   - Post for community comment
   - Revise based on feedback

3. **Publish**:
   - Update [ROADMAP.md](../../ROADMAP.md)
   - Announce in CHANGELOG
   - Communicate to pilot communities

**Timeline**: Quarterly (or after major milestones)

---

## Consensus Model

**We aim for consensus**, not majority vote.

### Consensus Guidelines

1. **Seek broad agreement**: Consider all perspectives
2. **Document dissent**: Minority opinions are valuable
3. **Disagree and commit**: Once decided, everyone supports it
4. **Revisit if needed**: New information can change decisions

### When Consensus Fails

If maintainers cannot reach consensus:
1. **Extended discussion**: Take more time, explore alternatives
2. **Pilot input**: Ask pilot communities for feedback
3. **Lead decision**: Lead maintainer makes final call
4. **Document rationale**: Explain why decision was made

**Last resort**: Benevolent dictator model (lead maintainer decides)

---

## Communication Channels

### GitHub Issues

**Purpose**: Bug reports, feature requests, RFCs

**Guidelines:**
- Search before creating (avoid duplicates)
- Use templates when provided
- Be respectful and constructive
- Tag appropriately (bug, enhancement, question, etc.)

### Pull Requests

**Purpose**: Code contributions

**Guidelines:**
- Reference related issue(s)
- Follow code style (see [CONTRIBUTING.md](../CONTRIBUTING.md))
- Include tests
- Update documentation
- Be responsive to feedback

### CHANGELOG

**Purpose**: User-facing change announcements

**Guidelines:**
- Semantic versioning
- Clear, concise descriptions
- Credit contributors
- Link to relevant issues/PRs

### Roadmap

**Purpose**: Strategic direction

**Guidelines:**
- Updated quarterly or after milestones
- Reflects pilot feedback
- Realistic timelines (or no timelines)
- Subject to change based on learnings

---

## Release Process

### Versioning

ICN follows [Semantic Versioning](https://semver.org/):
- **MAJOR**: Breaking changes (e.g., protocol version bump)
- **MINOR**: New features (backward-compatible)
- **PATCH**: Bug fixes (backward-compatible)

### Release Cadence

- **Patch releases**: As needed (security fixes, critical bugs)
- **Minor releases**: Monthly (when features accumulate)
- **Major releases**: Rare (only when breaking changes required)

### Release Checklist

1. **Prepare**:
   - [ ] Update CHANGELOG.md
   - [ ] Update version in Cargo.toml files
   - [ ] Run full test suite
   - [ ] Run clippy and fmt
   - [ ] Build release binaries

2. **Tag**:
   - [ ] Create git tag: `git tag -a v0.2.0 -m "Release 0.2.0"`
   - [ ] Push tag: `git push origin v0.2.0`

3. **Build**:
   - [ ] Build for multiple platforms (Linux x86_64, aarch64, macOS)
   - [ ] Generate checksums

4. **Publish**:
   - [ ] Create GitHub release
   - [ ] Upload binaries and checksums
   - [ ] Update installer script
   - [ ] Announce to pilot communities

5. **Verify**:
   - [ ] Test install script
   - [ ] Test upgrade path
   - [ ] Monitor for issues

---

## Code of Conduct Enforcement

We follow the [Contributor Covenant Code of Conduct](../CODE_OF_CONDUCT.md).

### Reporting

**How to report violations:**
- Email: [INSERT CONTACT]
- Private GitHub issue (if you have access)
- Direct message to maintainer

**What to include:**
- Description of violation
- Context (where, when, who)
- Any evidence (screenshots, logs)
- Your preferred outcome

### Response Process

1. **Acknowledgment**: Within 24 hours
2. **Investigation**: Gather facts, contact parties
3. **Decision**: Apply enforcement ladder (see Code of Conduct)
4. **Communication**: Inform reporter and subject
5. **Appeal**: Subject can appeal within 7 days

**Confidentiality**: Reporter identity protected unless they consent to disclosure.

---

## Conflict Resolution

If conflicts arise (technical disagreements, interpersonal issues):

1. **Direct communication**: Talk to each other first
2. **Maintainer mediation**: If direct communication fails
3. **Cooling-off period**: Take a break if emotions high
4. **Document positions**: Write down each perspective
5. **Find common ground**: Look for win-win solutions
6. **Escalate if needed**: Lead maintainer makes final call

**Remember**: We're building infrastructure for cooperation. Model the behavior we want to see in cooperatives.

---

## Pilot Community Partnership

### Relationship

- **Partnership, not vendor/client**: We build together
- **Feedback loops**: Weekly check-ins during pilot
- **Mutual learning**: We learn from them, they learn from ICN
- **Long-term alignment**: Not just a test, but a relationship

### Expectations

**What pilots provide:**
- Regular feedback (weekly during pilot)
- Bug reports and feature requests
- Documentation of use cases
- Patience with rough edges

**What ICN provides:**
- Responsive support
- Rapid iteration on feedback
- Training and onboarding
- Commitment to long-term success

### Decision Rights

**Pilots influence but don't control**:
- ✅ Pilots guide priorities (what to build next)
- ✅ Pilots validate designs (will this work for us?)
- ❌ Pilots don't dictate architecture (how to build it)
- ❌ Pilots don't veto security requirements

**Rationale**: ICN serves many cooperatives, not just one. We balance all pilot needs.

---

## Amendment Process

This governance document can be amended:

1. Open GitHub issue proposing change
2. Community discussion (2 weeks minimum)
3. Maintainer consensus
4. Update document
5. Announce in CHANGELOG

**Principle**: Governance evolves as project matures.

---

## Financial Sustainability (Future)

**Current state**: Volunteer-driven, no formal funding

**Future considerations** (to be determined):
- Grant funding (e.g., NGI, cooperative development orgs)
- Cooperative membership dues (hosted services?)
- Service contracts (support, training, customization)
- Donations (if community grows)

**Decision**: Deferred until after pilot deployments. Focus on building value first.

**Commitment**: ICN will remain open source (MIT/Apache-2.0) regardless of funding model.

---

## Legal Structure (Future)

**Current state**: Informal open source project

**Future considerations**:
- Nonprofit foundation (if grant funding needed)
- Cooperative corporation (if selling services)
- Platform cooperative (if hosting services)

**Decision**: Deferred until project matures. No legal entity needed for development.

---

## Trademark & Branding

**Status**: No registered trademarks yet

**Guidelines**:
- "ICN" and "Intercooperative Network" are project names
- Logo/branding TBD
- Anyone can use project name to describe the software
- Don't imply endorsement without permission
- Don't use name for unrelated products

**Future**: May register trademark if project grows to prevent misuse.

---

## Questions?

If you have questions about project governance:

1. Check this document
2. Search GitHub issues
3. Open a new issue tagged "governance"
4. Ask in community channels

**This is a living document.** Governance evolves with the project.

---

## History

- **2025-11-21**: Initial governance document created
- *Future amendments will be listed here*

---

**Thank you for being part of ICN!** Together, we're building infrastructure for a more cooperative future.
