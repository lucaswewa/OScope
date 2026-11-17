#!/usr/bin/env bash
# Gathers the server's third-party notices (P54, ADR-0033) in FOLDER
# [default: target/notices], for the packages:
#
# - THIRD-PARTY-NOTICES.html, each crate's licence, by cargo-about;
# - Roboto-LICENSE.txt, the fonts' licence;
# - typst-assets-0.15.1-NOTICE.txt, the data typst-assets bundles.
#
# Run it from the repository, with cargo-about 0.9.3 installed. It prints the
# folder.
set -euo pipefail

folder=${1:-target/notices}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$folder"
cargo about generate --locked -c "$here/about.toml" -m crates/oscope-server/Cargo.toml \
    "$here/about.hbs" -o "$folder/THIRD-PARTY-NOTICES.html"
cp crates/oscope-report/fonts/LICENSE "$folder/Roboto-LICENSE.txt"
cp "$here/typst-assets-0.15.1-NOTICE.txt" "$folder/"
echo "$folder"
