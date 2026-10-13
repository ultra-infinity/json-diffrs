#!/usr/bin/env bash
# Builds, signs and publishes a json-diff release, including the latest.json
# the in-app updater reads:
#
#   nix develop -c scripts/release.sh
#
# The version is the one in crates/json-diff/Cargo.toml. Needs the release
# signing key (README > Releasing) at $TAURI_SIGNING_PRIVATE_KEY_PATH, by
# default ~/.tauri/json-diff.key, and the GitHub CLI logged in. Nothing is
# published until you confirm.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=ultra-infinity/json-diffrs
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/json-diff/Cargo.toml | head -n 1)
tag="v$version"
msi="json-diff-$version-x86_64.msi"
key=${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/json-diff.key}

fail() {
  echo "release: $*" >&2
  exit 1
}
[[ -f $key ]] || fail "no signing key at $key (README > Releasing)"
grep -q '"pubkey": ""' crates/json-diff/tauri.conf.json &&
  fail "tauri.conf.json has no updater public key (README > Releasing)"
[[ -z $(git status --porcelain) ]] || fail "commit your changes first: releases are built from git"
gh release view "$tag" -R "$repo" > /dev/null 2>&1 && fail "$tag is already released; bump the version"

rm -rf dist
mkdir dist
nix build .#msi -o dist/msi
cp "dist/msi/$msi" dist/
rm dist/msi
# Writes dist/$msi.sig; asks for the key's password unless
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD is set.
cargo tauri signer sign --private-key-path "$key" "dist/$msi"
cat > dist/latest.json << JSON
{
  "version": "$version",
  "notes": "https://github.com/$repo/releases/tag/$tag",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "windows-x86_64": {
      "signature": "$(cat "dist/$msi.sig")",
      "url": "https://github.com/$repo/releases/download/$tag/$msi"
    }
  }
}
JSON
(cd dist && sha256sum "$msi" > SHA256SUMS)

echo
echo "Ready to release $tag:"
ls -1 dist
read -r -p "Tag $tag and publish it on GitHub now? [y/N] " answer
if [[ $answer != [yY] ]]; then
  echo "Not published; the files are in dist/."
  exit 0
fi
git tag -a "$tag" -m "json-diff $version"
git push origin "$tag"
gh release create "$tag" -R "$repo" --title "json-diff $version" --generate-notes \
  "dist/$msi" dist/latest.json dist/SHA256SUMS
