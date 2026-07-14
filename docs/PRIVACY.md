# Privacy & the public/private split

`scrybe` is **public**, Apache-2.0 software. Users run it against their own
files, workspaces, and (optionally) their own MCP agents and git remotes. A
public repo that leaks the operational specifics of the maintainer's — or any
user's — real environment hands an attacker a map. This document defines the
boundary and how it is enforced.

## The rule

- **Public (this repo):** generic, reusable code and documentation. Every
  example uses a **placeholder**, never a real value.
- **Private (operator/user-controlled):** the actual environment — real
  hostnames, addresses, internal domains, git remotes, accounts, and all
  secrets. This stays on the user's machine and **never** flows here.
- **Direction of authorship:** docs and tests are authored **placeholder-first**.
  Never copy-paste a real value from a running machine into the repo.

## Approved placeholders

| Category | Use this | Never this |
|---|---|---|
| Host / remote | `host.example.lan`, `gitea.example.lan` | your real hostnames |
| IP / CIDR | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` (RFC 5737 TEST-NET) | real RFC1918 / CGNAT addresses |
| DNS domain | `example.lan`, `example.com` | your real internal domain |
| Directory realm | `EXAMPLE.LAN`, base DN `dc=example,dc=lan` | your real realm / base DN |
| Overlay network | `<OVERLAY-NETWORK>` | your real mesh / tailnet name |
| User / email | `user@example.com` | real people / personal email providers |
| Secret reference | `secret/path/to/value` (a *reference*, not a value) | any real secret value |

## Public by design (not flagged)

The linter uses **generic** patterns and deliberately does **not** flag values
that are legitimately public in this project:

- The public owner/handle `hartsock` and `*.users.noreply.github.com` /
  `noreply@anthropic.com` commit addresses.
- Scrybe's own runtime paths on a user's machine: `~/.scrybe`, `~/venv`,
  `/tmp/scrybe-*`.
- Loopback / unspecified / mDNS placeholders used in code and tests:
  `127.0.0.1`, `0.0.0.0`, `localhost`, `*.local`.

## Forbidden categories (blocked by CI)

Real values in any of these must never appear in code, docs, tests, fixtures,
comments, or commit messages:

1. Hostnames, IP addresses (private / CGNAT / link-local), DNS names,
   overlay-network / tailnet names.
2. Directory realm, AD domain, LDAP base DN, NetBIOS name.
3. Personal email addresses, private group names.
4. Any secret material (passwords, API keys, tokens, OAuth client secrets,
   private keys, update-signing seeds).
5. Network topology, port maps, or service-discovery detail that maps an attack
   surface.

## Enforcement

- **CI** (`.github/workflows/security-audit.yml`): a secret scanner (gitleaks)
  and the **internal-specifics linter**
  (`scripts/check-internal-specifics.sh`, generic pattern set) run on every push
  and pull request. A finding blocks the merge.
- **Local** (`.pre-commit-config.yaml` + `.githooks/pre-push`): the same checks
  run before a commit is created and again before a push, so leaks are caught
  before they leave a workstation.

## If CI flags you

Replace the flagged value with the appropriate placeholder from the table above
and re-push. If you believe it is a false positive on a genuine documentation
example, prefer switching the example to an RFC 5737 / `example.*` value rather
than widening the linter.
