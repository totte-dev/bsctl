# bsctl

A CLI client and MCP server for [Backstage](https://backstage.io), built in Rust.

## Why bsctl?

Backstage has a great Web UI and an official CLI for plugin development, but **no management CLI** for day-to-day catalog operations. bsctl fills that gap.

| | bsctl | Backstage Web UI | @backstage/cli |
|---|---|---|---|
| Catalog CRUD | Yes | Yes | No |
| Template execution | Yes (with `--wait`) | Yes | No |
| Scriptable / CI-friendly | Yes | No | No |
| MCP for AI agents | Yes (client-side) | No | No |
| Custom commands per plugin | Yes (`.bsctl.yaml`) | No | No |
| Requires Backstage changes | No | — | — |

### vs @backstage/plugin-mcp-actions-backend

Backstage also has an [official MCP plugin](https://github.com/backstage/backstage/tree/master/plugins/mcp-actions-backend) (experimental) that runs as a server-side backend plugin. bsctl takes the opposite approach:

- **bsctl**: Client-side. `cargo install bsctl` and you're done. No Backstage deployment needed.
- **Official plugin**: Server-side. Requires installing a backend plugin and redeploying Backstage.

Both are useful — the official plugin is better for shared team MCP endpoints, bsctl is better for individual developer workflows and CI/CD.

## Features

- **Catalog** — List, get, register, unregister, refresh, facets with filtering, sorting, and pagination
- **Search** — Full-text search across the catalog
- **Templates** — Describe parameter schemas, run with `--wait`, cancel tasks
- **Auth** — Guest login, OAuth browser flow, static tokens, JWT expiry detection
- **Plugins** — Extend with custom commands via `.bsctl/plugins.yaml`
- **Custom Columns** — Per-type column views with `.bsctl/columns/`, auto-generated from entities
- **MCP** — 14 tools for AI agent integration, including custom plugin commands
- **Output** — Table (terminal-width-aware), JSON, jsonpath

## Install

```bash
cargo install bsctl
```

## Quick Start

```bash
# Configure and authenticate
bsctl config set-context dev --base-url http://localhost:7007
bsctl login -p guest

# Explore the catalog
bsctl catalog list
bsctl catalog list -t service --sort name
bsctl catalog get component:my-service
bsctl catalog facets spec.type
bsctl search query "payment"

# Work with templates
bsctl template list
bsctl template describe create-react-app
bsctl template run create-react-app -p name=my-app --wait
```

## Documentation

- [Command Reference](docs/commands.md) — All commands with examples
- [Authentication](docs/authentication.md) — Auth methods and configuration
- [Plugin System](docs/plugin-definition.md) — Custom commands via `.bsctl.yaml`
- [Custom Columns](docs/custom-columns.md) — Per-type column views

## MCP Server

```json
{
  "mcpServers": {
    "backstage": {
      "command": "bsctl",
      "args": ["mcp"],
      "env": { "BSCTL_BASE_URL": "http://localhost:7007" }
    }
  }
}
```

14 tools: `login`, `catalog_list`, `catalog_get`, `catalog_refresh`, `catalog_register`, `catalog_unregister`, `catalog_facets`, `search`, `template_list`, `template_describe`, `template_run`, `template_status`, `template_cancel`, `plugin_call`.

## License

MIT
