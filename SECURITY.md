# Security Policy

`scrybe` is a Markdown editor and MCP server, distributed as an open-source
(Apache-2.0) desktop app and a set of Rust/Python packages. Security and privacy
are enforced in CI before any change lands on `main`.

## Reporting a vulnerability

Open a private security advisory on this repository
(<https://github.com/hartsock/scrybe/security/advisories/new>), or contact the
maintainer out-of-band. Please do not file public issues for undisclosed
vulnerabilities.

## Public-repository privacy rules (enforced)

This repo is public. It must never contain the operational specifics of any real
deployment or workstation. The following are **prohibited in code, docs, tests,
fixtures, comments, and commit messages**:

- Real hostnames, IP addresses (RFC1918 / CGNAT / link-local), domains, DNS
  names, or overlay-network / tailnet names.
- Real Kerberos realms, AD domains, LDAP base DNs, or NetBIOS names.
- Personal email addresses (e.g. `@gmail.com`) or private group names.
- Any secret: passwords, API keys, tokens, OAuth client secrets, private keys,
  private halves of certificates, signing seeds, or update-signing private keys.
- Internal network topology, port maps, or service-discovery details that map an
  attack surface.

Use **placeholders only**: `host.example.lan`, `example.com`, `EXAMPLE.LAN`,
`dc=example,dc=lan`, `192.0.2.0/24` (TEST-NET-1), `198.51.100.0/24`,
`203.0.113.0/24`, `user@example.com`, `<OVERLAY-NETWORK>`,
`secret/path/to/value` (a reference form, never a value).

### Explicitly allowed (public by design)

These appear in the repo and are **not** secrets or internal specifics:

- The public owner/handle `hartsock` and GitHub `*.users.noreply.github.com` /
  `noreply@anthropic.com` commit addresses.
- Scrybe runtime paths on a user's own machine: `~/.scrybe`, `~/venv`,
  `/tmp/scrybe-*`.
- Loopback / unspecified addresses used in code and tests: `127.0.0.1`,
  `0.0.0.0`, `localhost`.

## Enforcement

- **`security-audit` CI** (`.github/workflows/security-audit.yml`, GitHub-hosted
  runners): a gitleaks secret scan plus the internal-specifics linter
  (`scripts/check-internal-specifics.sh`). A finding **blocks the merge**.
- **Pre-commit** (`.pre-commit-config.yaml`): the same checks run locally before a
  commit is created.
- **Pre-push hook** (`.githooks/pre-push`): mirrors the CI gate so a leak is
  caught before it ever reaches the remote (see AGENTS.md "CI / Hook Parity").

## Secret handling in code

- Secrets (tokens, API keys, update-signing keys) are never committed, never
  written to durable disk in the repo, and never logged.
- The desktop app's update-signing **private** key lives only in release CI
  secrets; only the public verification key is ever distributed.
- Least-privilege scopes for any third-party integration; tokens are revocable
  and stored outside the repository.
