# Command Reference

## Catalog

```bash
# List entities
bsctl catalog list                              # All entities
bsctl catalog list --kind Component             # Filter by kind
bsctl catalog list -t tenant                    # Filter by spec.type
bsctl catalog list -t tenant --sort name        # Sort by name/kind/type/owner
bsctl catalog list -t tenant --tag dev          # Filter by tag
bsctl catalog list --limit 100 --offset 500     # Pagination
bsctl catalog list -o json                      # JSON output
bsctl catalog list -o jsonpath=metadata.name    # Extract field values
bsctl catalog list -o jsonpath=$.spec.owner     # $ prefix supported

# Get entity details
bsctl catalog get component:my-service
bsctl catalog get resource:default/client-tc3 -o json

# Discover available values
bsctl catalog facets kind                       # All entity kinds with counts
bsctl catalog facets spec.type                  # All entity types with counts
bsctl catalog facets spec.lifecycle

# Register / Unregister
bsctl catalog register https://github.com/org/repo/blob/main/catalog-info.yaml
bsctl catalog unregister component:my-old-service

# Refresh
bsctl catalog refresh component:my-service
```

## Search

```bash
bsctl search query "tenant"                     # Search catalog
bsctl search query "api" -t software-catalog    # Filter by index type
bsctl search query "deploy" --limit 50          # Limit results
```

## Templates

```bash
# Browse
bsctl template list
bsctl template list -o json

# Inspect parameter schema
bsctl template describe my-template

# Run
bsctl template run my-template -p key1=value1 -p key2=value2
bsctl template run my-template -p key=value --wait              # Block until done
bsctl template run my-template -p key=value --wait --timeout 300  # Custom timeout

# Monitor
bsctl template status <task-id>
bsctl template log <task-id>
bsctl template cancel <task-id>
```

## Raw API

Escape hatch for any Backstage API endpoint:

```bash
bsctl api get /api/catalog/entities
bsctl api get /api/catalog/entities -q filter=kind=Component
bsctl api post /api/catalog/refresh -p entityRef=component:default/my-svc
bsctl api put /api/some/endpoint -b '{"key":"value"}'
bsctl api delete /api/catalog/locations/abc-123
```

## Configuration

```bash
bsctl config set-context dev --base-url http://localhost:7007
bsctl config set-context prod --base-url https://backstage.example.com --token my-token
bsctl config use-context prod
bsctl config get-contexts
bsctl config current-context
bsctl config delete-context old
```

Config file: `~/.config/bsctl/config.yaml`

```yaml
current-context: dev
contexts:
  dev:
    base-url: http://localhost:7007
  production:
    base-url: https://backstage.example.com
    token: ${BSCTL_TOKEN}  # Environment variable reference
```

## Plugin Commands

```bash
bsctl plugins                                   # List available plugin commands
bsctl terraform prs                             # Example: list Terraform PRs
bsctl costs get --account-id 123456789          # Example: AWS costs
```

## Custom Columns

```bash
bsctl columns generate -t client-account        # Preview column definitions
bsctl columns generate -t tenant --write         # Save to .bsctl/columns/
```

## Other

```bash
bsctl version                                   # Show version
bsctl completions bash >> ~/.bashrc             # Generate shell completions
bsctl completions zsh >> ~/.zshrc
bsctl --insecure catalog list                   # Skip TLS verification
```
