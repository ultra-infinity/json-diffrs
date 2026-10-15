#!/usr/bin/env bash
# Builds, signs and publishes a json-diff release: the Windows installer, the
# latest.json the in-app updater reads, and the web app as a zip for anyone
# who wants to host it themselves:
#
#   nix develop -c scripts/release.sh
#
# The version is the one in crates/json-diff/Cargo.toml, and its notes are
# its entry in CHANGELOG.md (`## <version> - <date>`). Needs the release
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

# The CHANGELOG.md entry for this version, without its heading.
notes=$(awk -v version="$version" '
  /^## / { if (found) exit; found = ($2 == version) ; next }
  found
' CHANGELOG.md | sed -e '/./,$!d')
[[ -n $notes ]] || fail "CHANGELOG.md has no entry for $version: rename Unreleased to '## $version - $(date +%Y-%m-%d)'"

rm -rf dist
mkdir dist
printf '%s\n' "$notes" > dist/notes.md
nix build .#msi -o dist/msi
cp "dist/msi/$msi" dist/
rm dist/msi
# Writes dist/$msi.sig; asks for the key's password unless
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD is set.
cargo tauri signer sign --private-key-path "$key" "dist/$msi"
cat > dist/latest.json << JSON
{
  "version": "$version",
  "notes": $(node -e 'process.stdout.write(JSON.stringify(require("fs").readFileSync(0, "utf8").trim()))' < dist/notes.md),
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "windows-x86_64": {
      "signature": "$(cat "dist/$msi.sig")",
      "url": "https://github.com/$repo/releases/download/$tag/$msi"
    }
  }
}
JSON
# The web app, ready to unzip and serve (README > Hosting the web app).
web_zip="json-diff-web-$version.zip"
nix build .#web-zip -o dist/web-zip
cp "dist/web-zip/$web_zip" dist/
rm dist/web-zip
(cd dist && sha256sum "$msi" "$web_zip" > SHA256SUMS)

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
gh release create "$tag" -R "$repo" --title "json-diff $version" --notes-file dist/notes.md \
  "dist/$msi" "dist/$web_zip" dist/latest.json dist/SHA256SUMS
