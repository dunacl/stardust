#!/usr/bin/env bash
#
# Security checks: nothing private in git, no secrets in git, no known
# vulnerabilities, unwanted licences or unknown sources in the dependencies.
#
#   private files  planning documents must not be tracked or anywhere in history
#   gitleaks       credentials in any commit
#   cargo-deny     RustSec advisories, licences and sources (policy: deny.toml)
#
# Usage: scripts/check-security.sh [--leaks]
#   --leaks  only the first two: fast, and what the pre-push hook runs.
#
# Needs gitleaks and cargo-deny (`cargo install --locked cargo-deny`). CI needs
# a full clone: a shallow one would hide the history.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck source=scripts/toolchain.sh
. scripts/toolchain.sh
[ -d "$HOME/go/bin" ] && PATH="$PATH:$HOME/go/bin"

need() {
  command -v "$1" >/dev/null || {
    echo "check-security: $1 is required ($2)" >&2
    exit 1
  }
}

PRIVATE='^(private/|PLAN\.md$|CLAUDE\.md$|docs/PROGRESS\.md$|docs/m6-preparation\.md$)'

found="$( {
  git ls-files
  git log --all --no-renames --format= --name-only
} | grep -E "$PRIVATE" | sort -u || true)"
if [ -n "$found" ]; then
  printf 'check-security: private files in git:\n%s\n' "$found" >&2
  exit 1
fi
echo "private files: none in git"

need gitleaks "https://github.com/gitleaks/gitleaks"
gitleaks git --no-banner --redact --log-level warn --log-opts=--all .
echo "gitleaks: no leaks"

[ "${1:-}" = "--leaks" ] && exit 0

command -v cargo-deny >/dev/null || need cargo-deny "cargo install --locked cargo-deny"
cargo deny --log-level error check advisories bans licenses sources
echo "cargo-deny: ok"
