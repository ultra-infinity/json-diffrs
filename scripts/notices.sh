#!/usr/bin/env bash
# Regenerates THIRD_PARTY_NOTICES.txt: the license of every library bundled
# into json-diff, Rust crates and the frontend's npm packages. Run it after
# changing dependencies (CI checks the file is current):
#
#   nix develop -c scripts/notices.sh
#
# Fails if a dependency's license is not in licenses/about.toml.
set -euo pipefail
cd "$(dirname "$0")/.."

(cd ui && npm ci --no-audit --no-fund --silent)
# The crates' license files are read from their sources.
cargo fetch --locked
cargo about generate --frozen -c licenses/about.toml -o THIRD_PARTY_NOTICES.txt licenses/about.hbs
node scripts/npm-notices.mjs >> THIRD_PARTY_NOTICES.txt
# Fonts embedded in exported PDFs (cargo-about only sees crates).
{
  printf '\nFonts\n-----\n\n%s\n' "$(printf '=%.0s' {1..80})"
  echo "DejaVu Sans Mono 2.37, regular and bold (crates/json-diff-app/fonts)"
  echo
  cat crates/json-diff-app/fonts/LICENSE-DejaVu
} >> THIRD_PARTY_NOTICES.txt
echo "wrote THIRD_PARTY_NOTICES.txt"
