#!/usr/bin/env bash
# Gathers the server's third-party notices (P54, ADR-0033) in FOLDER
# [default: target/notices], for the packages:
#
# - THIRD-PARTY-NOTICES.html, each crate's licence, by cargo-about;
# - Roboto-LICENSE.txt, the fonts' licence;
# - typst-assets-0.15.1-NOTICE.txt, the data typst-assets bundles.
#
# Run it from the repository, with cargo-about 0.9.3 installed for the cargo
# that runs it: under WSL, Linux's. Without it, it says how to install it. It
# prints the folder.
set -euo pipefail

# The release workflow's version.
about="cargo-about 0.9.3"
found=$(cargo about --version 2>/dev/null || true)
if [ "$found" != "$about" ]; then
    echo "make.sh needs $about, but this cargo has ${found:-none}. Install it with:" >&2
    echo "  cargo install cargo-about --version 0.9.3 --locked --features cli" >&2
    exit 1
fi

folder=${1:-target/notices}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$folder"
cargo about generate --locked -c "$here/about.toml" -m crates/oscope-server/Cargo.toml \
    "$here/about.hbs" -o "$folder/THIRD-PARTY-NOTICES.html"
cp crates/oscope-report/fonts/LICENSE "$folder/Roboto-LICENSE.txt"
cp "$here/typst-assets-0.15.1-NOTICE.txt" "$folder/"
echo "$folder"
