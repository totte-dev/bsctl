# Authentication

## Overview

bsctl resolves authentication tokens in this order:

1. `--token` CLI flag
2. `BSCTL_TOKEN` environment variable
3. `token` field in config context (`~/.config/bsctl/config.yaml`)
4. Saved credentials from `bsctl login` (`~/.config/bsctl/credentials.json`)

## Guest Auth (Local Development)

For Backstage instances with guest auth enabled:

```bash
bsctl login -p guest
```

This calls `GET /api/auth/guest/refresh` and saves the token. No browser required.

## OAuth Browser Flow

For real auth providers (GitHub, Google, Okta, Microsoft, etc.):

```bash
bsctl login -p github
bsctl login -p google
bsctl login -p microsoft
```

This:
1. Starts a local HTTP server on a random port
2. Opens your browser to `{base-url}/api/auth/{provider}/start`
3. After authentication, Backstage redirects back with a token
4. The token is saved to `~/.config/bsctl/credentials.json`

## Static Token

For CI/CD or service accounts using Backstage's `externalAccess` configuration:

```bash
# Via flag
bsctl --token my-static-token catalog list

# Via environment variable
export BSCTL_TOKEN=my-static-token
bsctl catalog list

# Via config file
bsctl config set-context ci --base-url https://backstage.example.com --token my-static-token
```

### Backstage Configuration

To enable static tokens on the Backstage side:

```yaml
# app-config.yaml
backend:
  auth:
    externalAccess:
      - type: static
        options:
          token: ${BSCTL_ACCESS_TOKEN}
          subject: bsctl-cli
```

## Token Expiry

bsctl automatically checks JWT expiry:

- **Expired tokens** are rejected with a message to re-run `bsctl login`
- **Tokens expiring within 5 minutes** show a warning
- **Static tokens** (non-JWT) are never considered expired

## Credential Storage

- Config: `~/.config/bsctl/config.yaml`
- Credentials: `~/.config/bsctl/credentials.json` (permissions: `0600`)
- Tokens from `bsctl login` are stored in credentials, not in config

Environment variable references (`${VAR}`) in config token fields are resolved at load time.
