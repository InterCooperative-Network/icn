# GitHub Copilot Instructions

This directory contains instructions that guide GitHub Copilot when working with code in this repository.

## Overview

GitHub Copilot uses these instruction files to understand the codebase structure, conventions, and best practices. This helps Copilot generate better code suggestions and complete tasks more effectively.

## Instruction Files

### Repository-Wide Instructions

**`.github/copilot-instructions.md`** - Main instructions that apply to the entire repository
- Project overview and architecture
- Repository structure
- Development workflow and commands
- Common development tasks
- Testing patterns
- Key files to reference

### Path-Specific Instructions

Path-specific instructions are located in `.github/instructions/` and apply to specific parts of the codebase using glob patterns.

#### `rust-core.md`
**Applies to:** `icn/crates/**/*.rs`

Instructions for Rust core crates including:
- Rust edition and version
- Code style and conventions
- Async/await patterns with Tokio
- Actor pattern implementation
- Error handling with thiserror
- Testing practices
- Concurrency and synchronization
- Security best practices
- Metrics and observability

#### `web-ui.md`
**Applies to:** `web/**/*.{js,html,css}`

Instructions for web frontend (vanilla JavaScript PWA):
- Technology stack (ES6+, HTML5, CSS3, PWA)
- Component patterns
- API client usage
- Error handling and user-friendly messages
- Testing with Jest and Playwright
- PWA conventions

#### `sdk.md`
**Applies to:** `sdk/**/*.{ts,tsx,js,jsx}`

Instructions for client SDKs (TypeScript and React Native):
- TypeScript strict mode conventions
- SDK design principles
- Type safety and error handling
- Testing requirements
- Documentation standards
- Build configuration

#### `documentation.md`
**Applies to:** `docs/**/*.md`

Instructions for writing documentation:
- Documentation philosophy
- Writing style and tone
- Markdown conventions
- Code example formatting
- Documentation types (architecture, API, guides)
- Maintenance practices

## How It Works

When GitHub Copilot works on a task, it:

1. **Reads repository-wide instructions** from `.github/copilot-instructions.md`
2. **Loads path-specific instructions** based on which files are being modified
3. **Combines all relevant instructions** to understand context and conventions
4. **Generates code and suggestions** following these guidelines

For example, when working on a Rust file in `icn/crates/icn-gossip/`, Copilot will use:
- The main `copilot-instructions.md` for general repository knowledge
- The `rust-core.md` instructions for Rust-specific patterns

## Path Pattern Syntax

Path-specific instructions use YAML frontmatter with `applyTo` field containing glob patterns:

```yaml
---
applyTo: "icn/crates/**/*.rs"
---
```

Supported patterns:
- `**` - Matches any directory path
- `*` - Matches any characters except `/`
- `*.rs` - Matches all Rust files
- `*.{ts,tsx}` - Matches TypeScript files with either extension

## Best Practices for Maintainers

### Keeping Instructions Up-to-Date

1. **Update with code changes**: When changing conventions, update instructions in the same PR
2. **Review regularly**: Quarterly review to ensure instructions match current practices
3. **Be specific**: Include concrete examples rather than abstract principles
4. **Stay concise**: Copilot works best with clear, focused instructions

### What to Include

✅ **Do include:**
- Coding conventions and style guides
- Architecture patterns specific to this codebase
- Common pitfalls and how to avoid them
- Testing requirements and patterns
- Security considerations
- Build and development commands

❌ **Don't include:**
- General programming knowledge
- Detailed API documentation (belongs in code comments)
- Task-specific instructions (belongs in issue descriptions)
- Sensitive information

### Writing Effective Instructions

```markdown
# Good Example
## Error Handling

Use the `thiserror` crate for error types:

\`\`\`rust
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Failed to connect to peer {did}")]
    ConnectionFailed { did: String },
}
\`\`\`

# Less Effective Example
## Error Handling

We use error handling in this project. Make sure to handle errors properly.
```

## Testing Instructions

To verify instructions are working:

1. **Assign an issue to @copilot** with a clear task description
2. **Review the generated PR** to ensure it follows conventions
3. **Check code quality** - does it match the patterns in the instructions?
4. **Provide feedback** in PR comments if adjustments are needed

## References

- [GitHub Copilot Documentation](https://docs.github.com/en/copilot)
- [Best Practices for Copilot Coding Agent](https://docs.github.com/en/copilot/tutorials/coding-agent/get-the-best-results)
- [Custom Instructions Guide](https://github.blog/changelog/2025-07-23-github-copilot-coding-agent-now-supports-instructions-md-custom-instructions/)

## Contributing

Found an instruction that needs updating? Please:

1. Update the instruction file(s)
2. Test with a sample Copilot task if possible
3. Submit a PR with clear description of changes
4. Update this README if adding new instruction files

---

**Last Updated**: 2026-01-08
**Maintained by**: ICN Development Team
