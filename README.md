# bsctl

A CLI client for [Backstage](https://backstage.io), built in Rust.

## Features

- **Catalog** - List, get, and refresh entities with smart filtering
- **Search** - Full-text search across the catalog
- **Templates** - List, run, and monitor software templates
- **Auth** - Guest login, OAuth browser flow, static tokens
- **Plugins** - Extend with custom commands via `.bsctl.yaml`
- **MCP** - Use as an MCP server for AI agent integration

## Install

```bash
# From source
cargo install --path .

# Or build locally
cargo build --release
```

## Quick Start

```bash
# 1. Configure a context
bsctl config set-context dev --base-url http://localhost:7007

# 2. Authenticate (guest for local dev)
bsctl login -p guest

# 3. Use it
bsctl catalog list
bsctl catalog list -t tenant
bsctl catalog get resource:client-tc3
bsctl search query "my-service"
bsctl template list
```

## Commands

### Catalog

```bash
bsctl catalog list                        # List all entities
bsctl catalog list --kind Component       # Filter by kind
bsctl catalog list -t tenant              # Filter by spec.type
bsctl catalog get component:my-service    # Get entity details
bsctl catalog get resource:client-tc3 -o json  # JSON output
bsctl catalog refresh component:my-service     # Refresh entity
```

### Search

```bash
bsctl search query "tenant"               # Search catalog
bsctl search query "api" -t software-catalog   # Filter by index type
```

### Templates

```bash
bsctl template list                        # List templates
bsctl template run tenant-creation \
  -p client_ref=resource:default/client-tc3 \
  -p tenant_type=dev \
  -p auth_type=cognito                     # Run a template
bsctl template status <task-id>            # Check progress
bsctl template log <task-id>               # View logs
```

### Raw API

```bash
bsctl api get /api/catalog/entities        # GET any endpoint
bsctl api get /api/catalog/entities -q filter=kind=Component
bsctl api post /api/catalog/refresh -p entityRef=component:default/my-svc
```

## Authentication

bsctl supports multiple auth methods:

| Method | Use case | Setup |
|--------|----------|-------|
| Guest | Local development | `bsctl login -p guest` |
| OAuth | Per-user auth (GitHub, Google, etc.) | `bsctl login -p github` |
| Static token | CI/CD, service accounts | `--token` or `BSCTL_TOKEN` env |

Token priority: `--token` flag > `BSCTL_TOKEN` env > config file > credentials from `bsctl login`.

Tokens are stored in `~/.config/bsctl/credentials.json` with `0600` permissions. JWT expiry is checked automatically.

## Configuration

Config is stored in `~/.config/bsctl/config.yaml`:

```yaml
current-context: dev
contexts:
  dev:
    base-url: http://localhost:7007
  production:
    base-url: https://backstage.example.com
    token: ${BSCTL_TOKEN}  # Environment variable reference
```

```bash
bsctl config set-context dev --base-url http://localhost:7007
bsctl config set-context prod --base-url https://backstage.example.com --token my-token
bsctl config use-context prod
bsctl config get-contexts
bsctl config current-context
bsctl config delete-context old
```

## Plugin System

Define custom commands for your Backstage plugins in `.bsctl.yaml` (project root or `~/.bsctl.yaml`):

```yaml
plugins:
  terraform:
    prs:
      method: GET
      path: /api/terraform-ops/infra-prs
      description: List infrastructure PRs
    pr:
      method: GET
      path: /api/terraform-ops/pr/{number}
      description: Get PR details
      args:
        - name: number
          position: 1
    merge:
      method: POST
      path: /api/terraform-ops/pr/{number}/merge
      args:
        - name: number
          position: 1
  costs:
    get:
      method: GET
      path: /api/aws-costs/costs
      description: AWS account costs
      params:
        - name: account-id
          query: accountId
          required: true
```

```bash
bsctl plugins                              # List all plugin commands
bsctl terraform prs                        # List PRs
bsctl terraform pr 42                      # PR details
bsctl terraform merge 42                   # Merge PR
bsctl costs get --account-id 123456789     # AWS costs
```

## Directory-Based Config

For larger projects, split config into a `.bsctl/` directory:

```
.bsctl/
  plugins.yaml              # Custom commands
  columns.ignore            # Annotation patterns to exclude
  columns/
    client-account.yaml     # Custom columns for client-account type
    tenant.yaml             # Custom columns for tenant type
```

Auto-generate column definitions from existing entities:

```bash
bsctl columns generate -t client-account         # Preview
bsctl columns generate -t client-account --write  # Save to .bsctl/columns/
```

Columns use dot-path syntax to extract annotation values:

```yaml
# .bsctl/columns/tenant.yaml
- header: Environment
  path: metadata.annotations.my-org.io/environment
  style: env  # dev=blue, preview=yellow, prod=green
- header: Customer
  path: metadata.annotations.my-org.io/customer
```

Exclude noisy annotations in `.bsctl/columns.ignore`:

```
*/terraform-path
*/suffix
backstage.io/*
```

## MCP Server

Use bsctl as an MCP server for AI agent integration:

```json
{
  "mcpServers": {
    "backstage": {
      "command": "bsctl",
      "args": ["mcp"],
      "env": {
        "BSCTL_BASE_URL": "http://localhost:7007"
      }
    }
  }
}
```

Available tools: `login`, `catalog_list`, `catalog_get`, `catalog_refresh`, `search`, `template_list`, `template_run`, `template_status`.

## Output Formats

All list commands support `-o json` for machine-readable output:

```bash
bsctl catalog list -o json | jq '.[].name'
bsctl catalog get resource:my-entity -o json
```

Table output auto-sizes columns to terminal width.

## License

MIT
