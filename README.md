# bsctl

A CLI client and MCP server for [Backstage](https://backstage.io), built in Rust.

## Features

- **Catalog** - List, get, register, unregister, refresh entities with filtering, sorting, and pagination
- **Search** - Full-text search across the catalog
- **Templates** - Describe parameter schemas, run with `--wait`, cancel tasks
- **Auth** - Guest login, OAuth browser flow, static tokens, JWT expiry detection
- **Plugins** - Extend with custom commands via `.bsctl/plugins.yaml`
- **Custom Columns** - Define per-type column views with `.bsctl/columns/`
- **MCP** - 14 tools for AI agent integration
- **Output** - Table (terminal-width-aware), JSON, jsonpath

## Install

```bash
cargo install --path .
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

- [Command Reference](docs/commands.md) - All commands with examples
- [Authentication](docs/authentication.md) - Auth methods and configuration
- [Plugin System](docs/plugin-definition.md) - Custom commands via `.bsctl.yaml`
- [Custom Columns](docs/custom-columns.md) - Per-type column views

## MCP Server

Use bsctl as an MCP server for AI agent integration:

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

14 tools available: `login`, `catalog_list`, `catalog_get`, `catalog_refresh`, `catalog_register`, `catalog_unregister`, `catalog_facets`, `search`, `template_list`, `template_describe`, `template_run`, `template_status`, `template_cancel`, `plugin_call`.

## License

MIT
