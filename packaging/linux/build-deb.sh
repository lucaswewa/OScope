#!/usr/bin/env bash
# Builds OScope's .deb (P54, ADR-0033) from a release build of the server:
#
#   packaging/linux/build-deb.sh SERVER [NOTICES] [FOLDER]
#
#   SERVER   the server, such as target/release/oscope-server
#   NOTICES  the third-party notices' folder, as packaging/notices/make.sh makes it
#   FOLDER   where the .deb goes [default: target/deb]
#
# It needs dpkg-deb and fakeroot, which Ubuntu has, and prints the .deb's path.
# Built on Ubuntu 22.04, as the release workflow builds it, it installs on
# 22.04 and 24.04.
set -euo pipefail

server=${1:?which server? packaging/linux/build-deb.sh SERVER [NOTICES] [FOLDER]}
notices=${2:-}
folder=${3:-target/deb}
here=$(cd "$(dirname "$0")" && pwd)
version=$("$server" --version | awk '{print $2}')
arch=$(dpkg --print-architecture)

root=$(mktemp -d)
chmod 755 "$root"
trap 'rm -rf "$root"' EXIT
install -D -m 755 "$server" "$root/usr/bin/oscope-server"
install -D -m 644 "$here/oscope.toml" "$root/etc/oscope/oscope.toml"
install -D -m 644 "$here/oscope.service" "$root/usr/lib/systemd/system/oscope.service"
install -D -m 644 "$here/oscope.rules" "$root/usr/share/polkit-1/rules.d/50-oscope.rules"
install -D -m 644 "$here/oscope.pkla" \
    "$root/var/lib/polkit-1/localauthority/10-vendor.d/50-oscope.pkla"
install -D -m 644 "$here/../../LICENSE" "$root/usr/share/doc/oscope/copyright"
if [ -n "$notices" ]; then
    install -m 644 "$notices"/* "$root/usr/share/doc/oscope/"
fi

mkdir -p "$root/DEBIAN"
for script in postinst prerm postrm; do
    install -m 755 "$here/debian/$script" "$root/DEBIAN/$script"
done
echo /etc/oscope/oscope.toml >"$root/DEBIAN/conffiles"
cat >"$root/DEBIAN/control" <<EOF
Package: oscope
Version: $version
Architecture: $arch
Maintainer: lucaswewa <lucaswewa@users.noreply.github.com>
Installed-Size: $(du -sk "$root" | cut -f1)
Depends: libc6 (>= 2.35), libgcc-s1, adduser
Recommends: polkitd | policykit-1
Section: science
Priority: optional
Homepage: https://github.com/lucaswewa/OScope
Description: OScope, the server of a motorised brightfield microscope
 Serves the instrument's Things, its media and its pages, as the account
 oscope, under systemd. Its configuration is /etc/oscope/oscope.toml, and its
 data /var/lib/oscope.
EOF

mkdir -p "$folder"
deb="$folder/oscope_${version}_${arch}.deb"
fakeroot dpkg-deb --build --root-owner-group "$root" "$deb" >/dev/null
echo "$deb"
