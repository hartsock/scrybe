// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! Remote roles and the configurable policy that assigns them.
//!
//! Scrybe never infers a remote's role from its host, brand, or port —
//! deployment topology is the *user's* policy, not the library's. Roles are
//! assigned by a [`RemoteRolePolicy`]: an ordered list of explicit rules
//! (exact remote-name match or URL glob) with a conventional-name fallback
//! (`origin` → [`RemoteRole::Origin`], `mirror` → [`RemoteRole::Mirror`],
//! `backup` → [`RemoteRole::Backup`], anything else → [`RemoteRole::Other`]).

use serde::{Deserialize, Serialize};

/// The role a remote plays in a multi-remote topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteRole {
    /// Source of truth; the default push/fetch target.
    Origin,
    /// Read-only mirror of the origin.
    Mirror,
    /// Backup copy of the origin.
    Backup,
    /// Any other remote; carries the remote's name (or a user-supplied label).
    Other(String),
}

/// How a single policy rule selects a remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteMatcher {
    /// Exact match on the remote's configured name (e.g. `"upstream"`).
    Name(String),
    /// Glob pattern matched against the remote's URL.
    ///
    /// `*` matches any run of characters (including none) and `?` matches
    /// exactly one character. The pattern must match the *entire* URL, so use
    /// leading/trailing `*` for substring-style matches
    /// (e.g. `"*@example.invalid:*"`).
    UrlGlob(String),
}

/// One ordered rule: a remote matched by [`RemoteMatcher`] gets `role`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRule {
    pub matcher: RemoteMatcher,
    pub role: RemoteRole,
}

/// Explicit, configurable mapping from remotes to [`RemoteRole`]s.
///
/// Rules are evaluated in order; the first match wins. When no rule matches,
/// the conventional-name fallback applies:
///
/// | remote name | role |
/// |---|---|
/// | `origin` | [`RemoteRole::Origin`] |
/// | `mirror` | [`RemoteRole::Mirror`] |
/// | `backup` | [`RemoteRole::Backup`] |
/// | anything else | [`RemoteRole::Other`]\(name\) |
///
/// [`RemoteRolePolicy::default()`] is the empty rule list — conventional
/// names only. No hosts, brands, or ports are ever baked in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRolePolicy {
    pub rules: Vec<RemoteRule>,
}

impl RemoteRolePolicy {
    /// Builds a policy from an ordered rule list (first match wins).
    pub fn new(rules: Vec<RemoteRule>) -> Self {
        Self { rules }
    }

    /// Classifies a remote by `name` and `url`.
    ///
    /// Explicit rules are checked in order; if none match, the
    /// conventional-name fallback documented on [`RemoteRolePolicy`] applies.
    pub fn classify(&self, name: &str, url: &str) -> RemoteRole {
        for rule in &self.rules {
            let matched = match &rule.matcher {
                RemoteMatcher::Name(n) => n == name,
                RemoteMatcher::UrlGlob(pattern) => glob_match(pattern, url),
            };
            if matched {
                return rule.role.clone();
            }
        }
        match name {
            "origin" => RemoteRole::Origin,
            "mirror" => RemoteRole::Mirror,
            "backup" => RemoteRole::Backup,
            other => RemoteRole::Other(other.to_owned()),
        }
    }
}

/// Matches `text` against a glob `pattern` (`*` = any run, `?` = one char).
///
/// Whole-string match with iterative `*` backtracking; no regex dependency.
fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None; // (pattern idx after '*', text idx)

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi + 1, ti));
            pi += 1;
        } else if let Some((star_pi, star_ti)) = star {
            // Backtrack: let the last '*' absorb one more character.
            pi = star_pi;
            ti = star_ti + 1;
            star = Some((star_pi, star_ti + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Named remote entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub name: String,
    pub url: String,
    /// Role assigned by the [`RemoteRolePolicy`] in effect.
    pub role: RemoteRole,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── default policy: conventional-name fallback ────────────────────────────

    #[test]
    fn test_default_policy_conventional_names() {
        let policy = RemoteRolePolicy::default();
        assert_eq!(
            policy.classify("origin", "git@example.invalid:x/y.git"),
            RemoteRole::Origin
        );
        assert_eq!(
            policy.classify("mirror", "https://example.invalid/x/y.git"),
            RemoteRole::Mirror
        );
        assert_eq!(
            policy.classify("backup", "ssh://git@example.invalid/x/y.git"),
            RemoteRole::Backup
        );
        assert_eq!(
            policy.classify("upstream", "https://example.invalid/x/y.git"),
            RemoteRole::Other("upstream".to_owned())
        );
    }

    #[test]
    fn test_default_policy_ignores_url_content() {
        // The URL never influences the fallback — only the remote name does.
        // (Regression guard: roles were once inferred from hard-coded host
        // and port substrings in the URL.)
        let policy = RemoteRolePolicy::default();
        assert_eq!(
            policy.classify("weird", "https://origin.example.invalid/mirror.git"),
            RemoteRole::Other("weird".to_owned())
        );
        assert_eq!(
            policy.classify("origin", "https://mirror.example.invalid/backup.git"),
            RemoteRole::Origin
        );
    }

    // ── explicit rules ────────────────────────────────────────────────────────

    #[test]
    fn test_name_rule_overrides_fallback() {
        let policy = RemoteRolePolicy::new(vec![RemoteRule {
            matcher: RemoteMatcher::Name("forge".to_owned()),
            role: RemoteRole::Origin,
        }]);
        assert_eq!(
            policy.classify("forge", "https://example.invalid/x/y.git"),
            RemoteRole::Origin
        );
        // A name rule is exact-match, not substring.
        assert_eq!(
            policy.classify("forge2", "https://example.invalid/x/y.git"),
            RemoteRole::Other("forge2".to_owned())
        );
        // Fallback still applies to unmatched remotes.
        assert_eq!(
            policy.classify("mirror", "https://example.invalid/x/y.git"),
            RemoteRole::Mirror
        );
    }

    #[test]
    fn test_url_glob_rule() {
        let policy = RemoteRolePolicy::new(vec![RemoteRule {
            matcher: RemoteMatcher::UrlGlob("*@example.invalid:*".to_owned()),
            role: RemoteRole::Mirror,
        }]);
        assert_eq!(
            policy.classify("anything", "git@example.invalid:x/y.git"),
            RemoteRole::Mirror
        );
        // Non-matching URL falls through to the conventional fallback.
        assert_eq!(
            policy.classify("anything", "https://other.invalid/x/y.git"),
            RemoteRole::Other("anything".to_owned())
        );
    }

    #[test]
    fn test_rules_first_match_wins() {
        let policy = RemoteRolePolicy::new(vec![
            RemoteRule {
                matcher: RemoteMatcher::UrlGlob("https://*".to_owned()),
                role: RemoteRole::Backup,
            },
            RemoteRule {
                matcher: RemoteMatcher::Name("origin".to_owned()),
                role: RemoteRole::Mirror,
            },
        ]);
        // Both rules match "origin" + https URL; the first (glob) wins.
        assert_eq!(
            policy.classify("origin", "https://example.invalid/x/y.git"),
            RemoteRole::Backup
        );
    }

    // ── glob matcher ──────────────────────────────────────────────────────────

    #[test]
    fn test_glob_match_semantics() {
        assert!(glob_match("*", "anything at all"));
        assert!(glob_match("*", ""));
        assert!(glob_match(
            "git@example.invalid:x/y.git",
            "git@example.invalid:x/y.git"
        ));
        assert!(glob_match("*.invalid/*", "host.invalid/x/y.git"));
        assert!(glob_match("ssh://*/y.git", "ssh://example.invalid/x/y.git"));
        assert!(glob_match("?it", "git"));
        // Whole-string semantics: a bare substring pattern does NOT match.
        assert!(!glob_match(
            "example.invalid",
            "git@example.invalid:x/y.git"
        ));
        assert!(!glob_match("*.invalid", "host.invalid/x/y.git"));
        assert!(!glob_match("?it", "gilt"));
        assert!(!glob_match("", "nonempty"));
        assert!(glob_match("", ""));
    }
}
