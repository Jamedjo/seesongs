#!/bin/sh
# Prepares a release in two steps, from the repository root:
#   scripts/release.sh 0.2.0        raises the crate to 0.2.0, for a release pull request
#   scripts/release.sh --tag 0.2.0  once that's merged, tags main v0.2.0 and pushes the tag, which
#                                   the Release workflow publishes as a GitHub release
set -e

usage() { echo "usage: $0 [--tag] <version>" >&2; exit 1; }

tag=
if [ "$1" = "--tag" ]; then tag=1; shift; fi
version=$1
echo "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || usage

current() { cargo pkgid | sed 's/.*[#@]//'; }

[ -z "$(git status --porcelain)" ] || { echo "The working tree has changes." >&2; exit 1; }
git fetch origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
    { echo "HEAD isn't origin/main." >&2; exit 1; }

if [ -n "$tag" ]; then
    [ "$(current)" = "$version" ] ||
        { echo "The crate is at $(current), not $version. Is the release merged?" >&2; exit 1; }
    git tag -a "v$version" -m "seesongs v$version"
    git push origin "v$version"
    exit 0
fi

old=$(current)
sed -i.bak "s/^version = \"$old\"/version = \"$version\"/" Cargo.toml
rm Cargo.toml.bak
cargo update --workspace
[ "$(current)" = "$version" ] || { echo "The crate didn't reach $version." >&2; exit 1; }
echo "Raised $old to $version. Once its pull request is merged: $0 --tag $version"
