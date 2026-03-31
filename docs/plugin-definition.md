# Plugin Definition Reference

bsctl can be extended with custom commands via a `.bsctl.yaml` file. This allows you to wrap any Backstage plugin's REST API as CLI commands without writing Rust code.

## File Location

bsctl searches for `.bsctl.yaml` in this order:

1. `.bsctl.yaml` in the current directory
2. `.bsctl.yml` in the current directory
3. `~/.bsctl.yaml`
4. `~/.bsctl.yml`

Placing it in your project root and committing to git shares it with your team.

## Schema

```yaml
plugins:
  <plugin-name>:
    <command-name>:
      method: GET | POST | PUT | DELETE
      path: /api/your-plugin/endpoint/{arg}
      description: Human-readable description
      args:          # Positional arguments (substituted into path)
        - name: arg
          position: 1
          required: true   # default: true
          description: What this arg is
      params:        # Named parameters (--flag value)
        - name: flag-name
          query: queryParamKey    # Maps to URL query parameter
          body: jsonBodyKey       # Maps to JSON body field
          required: false
          description: What this param does
```

## Fields

### Command Definition

| Field | Required | Description |
|-------|----------|-------------|
| `method` | Yes | HTTP method: `GET`, `POST`, `PUT`, or `DELETE` |
| `path` | Yes | API path. Use `{name}` for positional arg substitution |
| `description` | No | Shown in `bsctl plugins` and `--help` |
| `args` | No | Positional arguments that substitute into the path |
| `params` | No | Named parameters passed as query strings or JSON body |

### Arg Definition

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Arg name. Must match `{name}` placeholder in path |
| `position` | Yes | 1-based position in the command line |
| `required` | No | Default: `true` |
| `description` | No | Shown in help |

### Param Definition

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Flag name (used as `--name` on CLI) |
| `query` | No | Maps to a URL query parameter key |
| `body` | No | Maps to a JSON body field key |
| `required` | No | Default: `false` |
| `description` | No | Shown in help |

A param can have both `query` and `body` — the value will be sent in both places.

## Examples

### Simple GET

```yaml
plugins:
  status:
    health:
      method: GET
      path: /api/my-plugin/health
      description: Check plugin health
```

```bash
bsctl status health
```

### Path Arguments

```yaml
plugins:
  items:
    get:
      method: GET
      path: /api/items/{id}
      args:
        - name: id
          position: 1
```

```bash
bsctl items get 42
# → GET /api/items/42
```

### Query Parameters

```yaml
plugins:
  reports:
    costs:
      method: GET
      path: /api/reports/costs
      params:
        - name: account-id
          query: accountId
          required: true
        - name: months
          query: months
```

```bash
bsctl reports costs --account-id 123456 --months 3
# → GET /api/reports/costs?accountId=123456&months=3
```

### POST with Body

```yaml
plugins:
  actions:
    trigger:
      method: POST
      path: /api/actions/trigger
      params:
        - name: workflow
          body: workflowName
          required: true
        - name: env
          body: environment
```

```bash
bsctl actions trigger --workflow deploy --env production
# → POST /api/actions/trigger {"workflowName":"deploy","environment":"production"}
```
