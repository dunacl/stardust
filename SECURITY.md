# Security

Please report a vulnerability privately, through **Report a vulnerability** on
this repository's Security tab, rather than in a public issue. Include what is
affected and how to reproduce it.

Only the latest commit on `main` is supported.

## What is checked

`scripts/check-security.sh` runs on every push and pull request, and weekly so
that new advisories are noticed without a code change. It looks for credentials
in the whole history (gitleaks) and for known vulnerabilities, unwanted licences
and unknown sources in the dependencies (cargo-deny). The policy, including each
accepted advisory and the reason for it, is in [`deny.toml`](deny.toml).
