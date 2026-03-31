# Custom Columns

bsctl can show custom columns per entity type by reading annotation values from your catalog entities.

## Setup

### Auto-generate from existing entities

```bash
# Preview what columns would be generated
bsctl columns generate -t service

# Save directly to .bsctl/columns/
bsctl columns generate -t service --write
bsctl columns generate -t website --write
```

### Manual definition

Create files in `.bsctl/columns/<type>.yaml`:

```yaml
# .bsctl/columns/service.yaml
- header: On-Call
  path: metadata.annotations.pagerduty.com/service-id
- header: Grafana
  path: metadata.annotations.grafana.com/dashboard-url
- header: Language
  path: metadata.annotations.backstage.io/language
```

```yaml
# .bsctl/columns/website.yaml
- header: Environment
  path: metadata.annotations.example.com/environment
  style: env  # dev=blue, preview=yellow, prod=green
- header: URL
  path: metadata.annotations.example.com/url
```

## Path Syntax

Dot-separated paths into the entity JSON. For annotation keys containing dots (e.g., `pagerduty.com/service-id`), the resolver automatically tries joining remaining segments:

```
metadata.annotations.pagerduty.com/service-id
^^^^^^^^ ^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^^^^^^^
  |         |              |
  |         |              annotation key (joined: "pagerduty.com/service-id")
  |         object key
  object key
```

## Styles

The optional `style` field supports:
- `env` — Colors values by environment: `dev` (blue), `preview` (yellow), `prod` (green)

## Ignore File

Exclude noisy annotations from both `columns generate` and `catalog list` display:

```
# .bsctl/columns.ignore
backstage.io/*
*/managed-by-*
*/source-location
```

Patterns:
- `*suffix` — Matches annotation keys ending with `suffix`
- `prefix*` — Matches keys starting with `prefix`
- `exact-key` — Exact match

## File Structure

```
.bsctl/
  columns.ignore          # Exclude patterns
  columns/
    service.yaml          # Columns for -t service
    website.yaml          # Columns for -t website
```

When `catalog list -t <type>` is used and a matching column file exists, custom columns are shown. Otherwise, standard columns (Name, Kind, Type, Owner, Description) are displayed.

Custom columns also apply to the MCP `catalog_list` tool, returning compact JSON with only the defined fields.
