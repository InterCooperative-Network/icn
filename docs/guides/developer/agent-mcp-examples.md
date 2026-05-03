# Agent MCP examples (compact)

Illustrative JSON shapes for `icn_ops_*` tools. Fields match `ops/mcp/src/diagnostics/schema.ts` (`AGENT_TOOL_CONTRACT_VERSION`). Unknown future keys should be ignored.

## icn_ops_doctor — clean MCP layer

```json
{
  "severity": "ok",
  "summary": "Environment looks healthy for icn-ops MCP and local agent workflows.",
  "checks": [
    { "id": "node_modules", "severity": "ok", "message": "ops/mcp/node_modules present." },
    { "id": "dist", "severity": "ok", "message": "ops/mcp/dist/index.js present." }
  ],
  "suggested_repairs": []
}
```

## icn_ops_next_steps — clean gate

```json
{
  "severity": "ok",
  "summary": "Environment looks ready; pick a verification plan for your change area.",
  "recommended_steps": [
    {
      "title": "Proceed with scoped verification",
      "reason": "No blocking MCP issues detected; use icn_ops_verification_plan for area-specific checks.",
      "working_directory": "repo_root",
      "safety": "read_only",
      "priority": "low",
      "blocks_agent_work": false
    }
  ],
  "diagnosis_digest": {
    "doctor_severity": "ok",
    "doctor_error_checks": 0,
    "portability_script_ok": true
  }
}
```

## icn_ops_next_steps — missing `dist` only (`node_modules` present)

```json
{
  "severity": "warn",
  "recommended_steps": [
    {
      "title": "Build MCP TypeScript output",
      "reason": "ops/mcp/dist/index.js is missing; some workflows expect a prebuilt dist.",
      "command": "npm run build",
      "working_directory": "ops/mcp",
      "safety": "modifies_local",
      "priority": "medium",
      "blocks_agent_work": false
    }
  ],
  "diagnosis_digest": { "doctor_severity": "warn", "portability_script_ok": true }
}
```

## icn_ops_next_steps — missing `node_modules`

```json
{
  "severity": "error",
  "summary": "Address blocking MCP or dependency issues before deep agent work.",
  "recommended_steps": [
    {
      "title": "Install MCP dependencies",
      "reason": "ops/mcp/node_modules is missing; MCP server and tests cannot run.",
      "command": "npm ci",
      "working_directory": "ops/mcp",
      "safety": "modifies_local",
      "priority": "high",
      "blocks_agent_work": true
    }
  ],
  "diagnosis_digest": {
    "doctor_severity": "error",
    "doctor_summary": "…",
    "doctor_suggested_repairs_count": 1,
    "doctor_error_checks": 2,
    "doctor_warn_checks": 0,
    "env_warning_codes": ["missing_node_modules"],
    "state_missing_important": 0,
    "mcp_config_issues": 0,
    "portability_script_ok": true
  }
}
```

## icn_ops_next_steps — missing kubectl (non-blocking)

```json
{
  "severity": "warn",
  "summary": "Environment usable; resolve warnings when they affect your task.",
  "recommended_steps": [
    {
      "title": "Kubernetes CLI unavailable",
      "reason": "kubectl missing or not configured; cluster-oriented MCP tools are limited.",
      "working_directory": "repo_root",
      "safety": "read_only",
      "priority": "low",
      "blocks_agent_work": false
    }
  ],
  "diagnosis_digest": { "doctor_severity": "ok", "doctor_summary": "…", "portability_script_ok": true }
}
```

## icn_ops_next_steps — dirty worktree

```json
{
  "severity": "warn",
  "recommended_steps": [
    {
      "title": "Review dirty worktree",
      "reason": "Uncommitted changes span 3 path(s); confirm scope before editing.",
      "command": "git status --short",
      "working_directory": "repo_root",
      "safety": "read_only",
      "priority": "medium",
      "blocks_agent_work": false
    }
  ],
  "diagnosis_digest": { "doctor_severity": "warn", "doctor_summary": "…" }
}
```

## icn_ops_verification_plan — `area: mcp`, `risk_level: standard`

```json
{
  "area": "mcp",
  "risk_level": "standard",
  "steps": [
    {
      "order": 1,
      "command": "npm run build",
      "working_directory": "ops/mcp",
      "purpose": "Typecheck and compile MCP server",
      "expected_success_signal": "tsc completes with exit code 0",
      "safety": "modifies_local",
      "estimated_runtime": "quick"
    }
  ]
}
```

## icn_ops_repo_map — sparse checkout

```json
{
  "entries": [
    {
      "path": "icn",
      "present": false,
      "kind": "directory",
      "description": "Rust workspace root (Cargo.toml).",
      "agent_use": "All cargo * commands run from here per AGENTS.md."
    },
    {
      "path": "website",
      "present": false,
      "kind": "directory",
      "description": "Public site source (if present).",
      "agent_use": "Content and build; not the Rust daemon.",
      "caution": "Absence is normal on sparse checkouts."
    }
  ]
}
```

## icn_ops_tool_schemas (meta)

```json
{
  "contract_version": "1.0.0",
  "unknown_fields_policy": "Clients MUST ignore unknown JSON keys…",
  "tools": [
    {
      "tool": "icn_ops_environment_report",
      "version": "1.0.0",
      "input_schema": null,
      "input_schema_summary": "No tool arguments (empty object).",
      "output_schema_summary": "EnvironmentReport{repoRoot,warnings[],…}",
      "stability": "stable"
    }
  ]
}
```

See [agent-mcp-tooling.md](./agent-mcp-tooling.md) for tool list and policy.
