# Custom Columns

bsctl can show custom columns per entity type by reading annotation values from your catalog entities.

## Setup

### Auto-generate from existing entities

```bash
# Preview what columns would be generated
bsctl columns generate -t client-account

# Save directly to .bsctl/columns/
bsctl columns generate -t client-account --write
bsctl columns generate -t tenant --write
```

### Manual definition

Create files in `.bsctl/columns/<type>.yaml`:

```yaml
# .bsctl/columns/tenant.yaml
- header: Environment
  path: metadata.annotations.my-org.io/environment
  style: env  # dev=blue, preview=yellow, prod=green
- header: Customer
  path: metadata.annotations.my-org.io/customer
- header: Account ID
  path: metadata.annotations.my-org.io/account-id
```

## Path Syntax

Dot-separated paths into the entity JSON. For annotation keys containing dots (e.g., `my-org.io/key`), the resolver automatically tries joining remaining segments:

```
metadata.annotations.my-org.io/customer
^^^^^^^^ ^^^^^^^^^^^ ^^^^^^^^^^^^^^^^^^
  |         |              |
  |         |              annotation key (joined: "my-org.io/customer")
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
*/terraform-path
*/suffix
backstage.io/*
my-org.io/internal-*
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
    client-account.yaml   # Columns for -t client-account
    tenant.yaml           # Columns for -t tenant
    shared-account.yaml   # Columns for -t shared-account
```

When `catalog list -t <type>` is used and a matching column file exists, custom columns are shown. Otherwise, standard columns (Name, Kind, Type, Owner, Description) are displayed.

Custom columns also apply to the MCP `catalog_list` tool, returning compact JSON with only the defined fields.
