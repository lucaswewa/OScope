#!/usr/bin/env bash
# The .deb's example (P54): installs it in clean Ubuntu containers, and checks
# what it made, that its server answers as its account with its
# configuration, and that purging it keeps its data:
#
#   packaging/linux/test-deb.sh DEB [IMAGE...]     [default: ubuntu:24.04]
#
# It needs Docker, and ends with "test-deb: ok". A .deb built on 24.04 needs
# its newer C library: the release workflow builds on 22.04 and tries both.
set -euo pipefail

deb=$(realpath "${1:?which .deb? packaging/linux/test-deb.sh DEB [IMAGE...]}")
shift
images=("${@:-ubuntu:24.04}")
for image in "${images[@]}"; do
    echo "test-deb: $image"
    docker run --rm -v "$deb:/tmp/oscope.deb:ro" "$image" bash -euc '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq >/dev/null
        apt-get install -y -qq /tmp/oscope.deb >/dev/null
        # What it made: the account, its folders, the unit, the polkit rules.
        id oscoped
        test "$(stat -c %U:%G:%a /var/lib/oscope)" = oscoped:oscoped:750
        test "$(stat -c %G:%a /etc/oscope/oscope.toml)" = oscoped:664
        grep -q "^User=oscoped$" /usr/lib/systemd/system/oscope.service
        grep -q "^SupplementaryGroups=dialout$" /usr/lib/systemd/system/oscope.service
        grep -q "manage-units" /usr/share/polkit-1/rules.d/50-oscope.rules
        test -f /var/lib/polkit-1/localauthority/10-vendor.d/50-oscope.pkla
        test -f /usr/share/doc/oscope/copyright
        oscope-server --version
        # Its server, as its account, with its configuration, answers.
        runuser -u oscoped -- oscope-server --config /etc/oscope/oscope.toml \
            >/tmp/server.log 2>&1 &
        server=$!
        for _ in $(seq 600); do
            (exec 3<>/dev/tcp/127.0.0.1/8210) 2>/dev/null && break
            kill -0 "$server" 2>/dev/null || { cat /tmp/server.log; exit 1; }
            sleep 0.1
        done
        exec 3<>/dev/tcp/127.0.0.1/8210
        printf "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n" >&3
        read -r status <&3
        case "$status" in *" 200 "*) ;; *) echo "$status"; cat /tmp/server.log; exit 1 ;; esac
        test -d /var/lib/oscope/data
        kill "$server"
        wait "$server" || true
        # Purged: its program and configuration go, its data stays.
        apt-get purge -y -qq oscope >/dev/null
        test ! -e /usr/bin/oscope-server && test ! -e /etc/oscope
        test -d /var/lib/oscope/data
        echo "  $(. /etc/os-release && echo "$PRETTY_NAME"): ok"
    '
done
echo "test-deb: ok"
