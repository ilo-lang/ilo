/// CLI capability flags — Deno-style operator sandbox.
///
/// When any `--allow-*` flag is present the policy switches from permissive
/// (the default, matching pre-0.13 behaviour) to restrictive: every IO
/// builtin checks whether the target host/path/command falls inside the
/// caller-supplied allowlist before executing. An empty allowlist (`""`)
/// means *no* targets are permitted; `"*"` means all targets are permitted.
///
/// # Default behaviour
///
/// `Caps::default()` returns `Caps::Permissive`, which **never** blocks.
/// This preserves full backwards compatibility: scripts that do not pass any
/// `--allow-*` flags continue to run without restriction.
///
/// # Example
///
/// ```
/// use ilo::caps::{Caps, Policy};
///
/// // Block all network access
/// let caps = Caps::Restricted {
///     net:   Policy::List(vec![]),
///     read:  Policy::All,
///     write: Policy::All,
///     run:   Policy::All,
/// };
/// assert!(caps.check_net("evil.example").is_err());
/// assert!(caps.check_net("good.example").is_err()); // still blocked — list is empty
/// ```
/// Allowlist policy for a single capability dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Policy {
    /// No restriction — all targets are allowed (the legacy default).
    All,
    /// Only targets in this list are permitted.  An empty list blocks everything.
    List(Vec<String>),
}

/// Per-process capability set derived from `--allow-*` CLI flags.
#[derive(Debug, Clone, Default)]
pub enum Caps {
    /// No `--allow-*` flags were passed; all IO is permitted (legacy behaviour).
    #[default]
    Permissive,
    /// At least one `--allow-*` flag was passed; enforce the sub-policies.
    Restricted {
        net: Policy,
        read: Policy,
        write: Policy,
        run: Policy,
    },
}

impl Caps {
    /// Parse a comma-separated `--allow-*` value.
    ///
    /// `"*"` or `"all"` → `Policy::All`.
    /// `""` → `Policy::List([])` (block everything).
    /// `"a,b,c"` → `Policy::List(["a","b","c"])`.
    pub fn parse_allow(val: &str) -> Policy {
        let trimmed = val.trim();
        if trimmed == "*" || trimmed.eq_ignore_ascii_case("all") {
            return Policy::All;
        }
        let items: Vec<String> = if trimmed.is_empty() {
            vec![]
        } else {
            trimmed
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        };
        Policy::List(items)
    }

    // ── Check helpers ─────────────────────────────────────────────────────────

    /// Returns `Ok(())` if a network request to `url` is allowed, or an
    /// `Err` with a human-readable policy message.
    pub fn check_net(&self, url: &str) -> Result<(), String> {
        let Caps::Restricted { net, .. } = self else {
            return Ok(());
        };
        match net {
            Policy::All => Ok(()),
            Policy::List(allowed) => {
                let host = extract_host(url);
                if allowed.iter().any(|h| host_matches(h, host)) {
                    Ok(())
                } else {
                    Err(format!(
                        "blocked by --allow-net policy: host={host} is not in the allowlist"
                    ))
                }
            }
        }
    }

    /// Returns `Ok(())` if reading `path` is allowed.
    pub fn check_read(&self, path: &str) -> Result<(), String> {
        let Caps::Restricted { read, .. } = self else {
            return Ok(());
        };
        match read {
            Policy::All => Ok(()),
            Policy::List(allowed) => {
                if allowed.iter().any(|prefix| path_matches(prefix, path)) {
                    Ok(())
                } else {
                    Err(format!(
                        "blocked by --allow-read policy: path={path} is not in the allowlist"
                    ))
                }
            }
        }
    }

    /// Returns `Ok(())` if writing `path` is allowed.
    pub fn check_write(&self, path: &str) -> Result<(), String> {
        let Caps::Restricted { write, .. } = self else {
            return Ok(());
        };
        match write {
            Policy::All => Ok(()),
            Policy::List(allowed) => {
                if allowed.iter().any(|prefix| path_matches(prefix, path)) {
                    Ok(())
                } else {
                    Err(format!(
                        "blocked by --allow-write policy: path={path} is not in the allowlist"
                    ))
                }
            }
        }
    }

    /// Returns `Ok(())` if executing `cmd` is allowed.
    pub fn check_run(&self, cmd: &str) -> Result<(), String> {
        let Caps::Restricted { run, .. } = self else {
            return Ok(());
        };
        match run {
            Policy::All => Ok(()),
            Policy::List(allowed) => {
                // Basename comparison: if `cmd` is `/usr/bin/ls`, also check `ls`.
                let base = cmd.rsplit('/').next().unwrap_or(cmd);
                if allowed.iter().any(|c| c == cmd || c == base) {
                    Ok(())
                } else {
                    Err(format!(
                        "blocked by --allow-run policy: cmd={cmd} is not in the allowlist"
                    ))
                }
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract the host portion from a URL string.
///
/// Strips the scheme (`https://`, `http://`), then takes everything up to the
/// first `/`, `?`, or `#`.  Falls back to the full string when no scheme is
/// present, which covers raw hostnames like `"example.com"`.
fn extract_host(url: &str) -> &str {
    // Strip scheme.
    let rest = if let Some(s) = url.strip_prefix("https://") {
        s
    } else if let Some(s) = url.strip_prefix("http://") {
        s
    } else {
        url
    };
    // Trim path/query/fragment.
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// True if `pattern` matches `host`.
///
/// Supports a leading `*.` wildcard (e.g. `*.example.com` matches
/// `api.example.com` and `example.com`).  Exact match is always tried first.
fn host_matches(pattern: &str, host: &str) -> bool {
    if pattern == host {
        return true;
    }
    // Wildcard: `*.example.com` matches `sub.example.com` and `example.com`.
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    false
}

/// True if `prefix` is a path prefix of `path`.
///
/// Empty prefix blocks everything (empty string is never a prefix of a
/// non-empty path via this check).  An exact match is always permitted.
/// Trailing-slash normalisation: `/tmp` is a prefix of `/tmp/foo` but not
/// `/tmpfoo`.
fn path_matches(prefix: &str, path: &str) -> bool {
    if prefix.is_empty() {
        return false;
    }
    if path == prefix {
        return true;
    }
    // prefix=/tmp  path=/tmp/foo → true
    // prefix=/tmp  path=/tmpfoo  → false
    let with_sep = if prefix.ends_with('/') {
        prefix.to_owned()
    } else {
        format!("{prefix}/")
    };
    path.starts_with(&with_sep)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_allow ───────────────────────────────────────────────────────────

    #[test]
    fn parse_star_is_all() {
        assert_eq!(Caps::parse_allow("*"), Policy::All);
    }

    #[test]
    fn parse_all_word_is_all() {
        assert_eq!(Caps::parse_allow("all"), Policy::All);
    }

    #[test]
    fn parse_empty_is_empty_list() {
        assert_eq!(Caps::parse_allow(""), Policy::List(vec![]));
    }

    #[test]
    fn parse_single_item() {
        assert_eq!(
            Caps::parse_allow("example.com"),
            Policy::List(vec!["example.com".to_owned()])
        );
    }

    #[test]
    fn parse_comma_separated() {
        assert_eq!(
            Caps::parse_allow("a.com,b.com,c.com"),
            Policy::List(vec![
                "a.com".to_owned(),
                "b.com".to_owned(),
                "c.com".to_owned()
            ])
        );
    }

    #[test]
    fn parse_whitespace_trimmed() {
        assert_eq!(
            Caps::parse_allow(" a.com , b.com "),
            Policy::List(vec!["a.com".to_owned(), "b.com".to_owned()])
        );
    }

    // ── default is permissive ─────────────────────────────────────────────────

    #[test]
    fn default_caps_permit_everything() {
        let caps = Caps::default();
        assert!(caps.check_net("https://evil.example").is_ok());
        assert!(caps.check_read("/etc/passwd").is_ok());
        assert!(caps.check_write("/etc/passwd").is_ok());
        assert!(caps.check_run("rm").is_ok());
    }

    // ── check_net ─────────────────────────────────────────────────────────────

    #[test]
    fn net_all_policy_allows_any() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::List(vec![]),
            write: Policy::List(vec![]),
            run: Policy::List(vec![]),
        };
        assert!(caps.check_net("https://evil.example").is_ok());
    }

    #[test]
    fn net_empty_list_blocks_all() {
        let caps = Caps::Restricted {
            net: Policy::List(vec![]),
            read: Policy::All,
            write: Policy::All,
            run: Policy::All,
        };
        let err = caps.check_net("https://example.com").unwrap_err();
        assert!(err.contains("--allow-net"), "msg={err}");
        assert!(err.contains("example.com"), "msg={err}");
    }

    #[test]
    fn net_allowlisted_host_passes() {
        let caps = Caps::Restricted {
            net: Policy::List(vec!["example.com".to_owned()]),
            read: Policy::All,
            write: Policy::All,
            run: Policy::All,
        };
        assert!(caps.check_net("https://example.com/api").is_ok());
    }

    #[test]
    fn net_non_allowlisted_host_blocked() {
        let caps = Caps::Restricted {
            net: Policy::List(vec!["example.com".to_owned()]),
            read: Policy::All,
            write: Policy::All,
            run: Policy::All,
        };
        let err = caps.check_net("https://evil.example").unwrap_err();
        assert!(err.contains("--allow-net"), "msg={err}");
    }

    #[test]
    fn net_wildcard_subdomain() {
        let caps = Caps::Restricted {
            net: Policy::List(vec!["*.example.com".to_owned()]),
            read: Policy::All,
            write: Policy::All,
            run: Policy::All,
        };
        assert!(caps.check_net("https://api.example.com/data").is_ok());
        assert!(caps.check_net("https://example.com/data").is_ok());
        assert!(caps.check_net("https://other.org").is_err());
    }

    // ── check_read ────────────────────────────────────────────────────────────

    #[test]
    fn read_empty_list_blocks_all() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::List(vec![]),
            write: Policy::All,
            run: Policy::All,
        };
        assert!(caps.check_read("/etc/passwd").is_err());
    }

    #[test]
    fn read_prefix_allows_subpath() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::List(vec!["/tmp".to_owned()]),
            write: Policy::All,
            run: Policy::All,
        };
        assert!(caps.check_read("/tmp/foo.txt").is_ok());
        assert!(caps.check_read("/tmp").is_ok());
    }

    #[test]
    fn read_prefix_does_not_allow_sibling() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::List(vec!["/tmp".to_owned()]),
            write: Policy::All,
            run: Policy::All,
        };
        assert!(caps.check_read("/tmpfoo").is_err());
        assert!(caps.check_read("/etc/passwd").is_err());
    }

    // ── check_write ───────────────────────────────────────────────────────────

    #[test]
    fn write_prefix_enforced() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::All,
            write: Policy::List(vec!["/tmp".to_owned()]),
            run: Policy::All,
        };
        assert!(caps.check_write("/tmp/out.txt").is_ok());
        assert!(caps.check_write("/etc/passwd").is_err());
    }

    // ── check_run ─────────────────────────────────────────────────────────────

    #[test]
    fn run_empty_list_blocks_all() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::All,
            write: Policy::All,
            run: Policy::List(vec![]),
        };
        assert!(caps.check_run("ls").is_err());
    }

    #[test]
    fn run_allowlisted_cmd_passes() {
        let caps = Caps::Restricted {
            net: Policy::All,
            read: Policy::All,
            write: Policy::All,
            run: Policy::List(vec!["ls".to_owned()]),
        };
        assert!(caps.check_run("ls").is_ok());
        assert!(caps.check_run("/usr/bin/ls").is_ok()); // basename match
        assert!(caps.check_run("rm").is_err());
    }

    // ── extract_host ──────────────────────────────────────────────────────────

    #[test]
    fn extract_host_https() {
        assert_eq!(extract_host("https://example.com/path?q=1"), "example.com");
    }

    #[test]
    fn extract_host_http() {
        assert_eq!(extract_host("http://api.example.com"), "api.example.com");
    }

    #[test]
    fn extract_host_no_scheme() {
        assert_eq!(extract_host("example.com"), "example.com");
    }

    // ── path_matches ──────────────────────────────────────────────────────────

    #[test]
    fn path_matches_exact() {
        assert!(path_matches("/tmp", "/tmp"));
    }

    #[test]
    fn path_matches_sub() {
        assert!(path_matches("/tmp", "/tmp/foo/bar.txt"));
    }

    #[test]
    fn path_matches_no_sibling() {
        assert!(!path_matches("/tmp", "/tmpfoo"));
    }

    #[test]
    fn path_matches_empty_prefix_never() {
        assert!(!path_matches("", "/anything"));
    }

    #[test]
    fn path_matches_trailing_slash_prefix() {
        assert!(path_matches("/tmp/", "/tmp/foo"));
    }
}
