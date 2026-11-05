#!/usr/bin/env bash
set -euo pipefail
repo=$(realpath "${1:?Usage: build-lxc.sh REPOSITORY_ROOT OUTPUT_DIRECTORY}")
out=$(realpath -m "${2:?Output directory required}")
[[ $(uname -m) == x86_64 ]] || { echo 'Use an amd64 builder.' >&2; exit 1; }
for tool in docker python3 curl zstd gpgv; do command -v "$tool" >/dev/null; done
mkdir -p "$out"
work=$(mktemp -d "$out/context.XXXXXX")
export_container=''
cleanup(){ if [[ -n "$export_container" ]]; then docker rm "$export_container" >/dev/null; fi; rm -rf "$work"; }
trap cleanup EXIT
commit=${SOURCE_COMMIT:-$(git -C "$repo" rev-parse HEAD)}
[[ $commit =~ ^[0-9a-f]{40}$ ]] || { echo 'A full source commit is required.' >&2; exit 1; }
base='ubuntu-24.04-standard_24.04-2_amd64.tar.zst'
expected='45c2978e6b97fe292ada95fe06834276015e5739a594db4de2fdfd830fa0c37942e8ae118fc1e32ffd9154b3f9378b592738b668ea3957db41f2907b86f219de'
curl -fsSL http://download.proxmox.com/images/aplinfo-pve-9.dat -o "$out/aplinfo.dat"
curl -fsSL http://download.proxmox.com/images/aplinfo-pve-9.dat.asc -o "$out/aplinfo.dat.asc"
curl -fsSL https://enterprise.proxmox.com/debian/proxmox-archive-keyring-trixie.gpg -o "$out/proxmox-keyring.gpg"
gpgv --keyring "$out/proxmox-keyring.gpg" "$out/aplinfo.dat.asc" "$out/aplinfo.dat"
python3 - "$out/aplinfo.dat" "$base" "$expected" <<'PY'
import sys
from pathlib import Path
blocks=Path(sys.argv[1]).read_text().split('\n\n')
assert any('Location: system/'+sys.argv[2] in b and 'sha512sum: '+sys.argv[3] in b for b in blocks),'Pinned base missing from signed catalog'
PY
if [[ ! -f "$out/$base" ]]; then curl -fsSL "http://download.proxmox.com/images/system/$base" -o "$out/$base"; fi
printf '%s  %s\n' "$expected" "$out/$base" | sha512sum -c -
zstd -dc "$out/$base" | docker import - neofand-lxc-base:ubuntu24.04
cp "$repo/dashboard/lxc/"{Dockerfile,Caddyfile,README.md,neofand-*.service} "$work/"
cp "$repo/dashboard/linux/linux_api.py" "$work/"
mkdir -p "$work/www/assets"
cp -a "$repo/dashboard/assets/." "$work/www/assets/"
# Never carry a downloaded distribution or host telemetry into a new image.
rm -rf "$work/www/assets/downloads"
cp "$repo/dashboard/linux/"{p620.js,p620.css,proxmox.js} "$work/www/assets/"
python3 "$repo/dashboard/linux/build_p620.py" "$repo/dashboard/index.html" "$work/www/index.html"
python3 - "$work/www/assets/build-info.json" "$commit" "${PR_NUMBER:-}" <<'PY'
import json,sys,datetime
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps(dict(commit=sys.argv[2],branch='codex/sensor-callouts',pr_number=int(sys.argv[3]) if sys.argv[3] else None,built_at=datetime.datetime.now(datetime.timezone.utc).isoformat())))
PY
docker pull caddy:2-alpine
caddy_image=$(docker image inspect caddy:2-alpine --format '{{index .RepoDigests 0}}')
image="neofand-lxc:${commit:0:7}"
docker build --build-arg "CADDY_IMAGE=$caddy_image" -t "$image" "$work"
# Configuration and rootfs checks do not need privileged mode or a running init.
docker run --rm --entrypoint /bin/sh "$image" -ec 'test -x /sbin/init; test ! -s /etc/machine-id; test ! -e /etc/ssh/ssh_host_ed25519_key; test -L /etc/systemd/system/multi-user.target.wants/neofand-api.service; test -L /etc/systemd/system/multi-user.target.wants/neofand-web.service; python3 -B -m py_compile /opt/neofand/linux_api.py; caddy validate --config /etc/neofand/Caddyfile --adapter caddyfile'
name="neofand-ubuntu-24.04_$(date -u +%Y%m%d)-${commit:0:7}_amd64.tar.zst"
export_container=$(docker create "$image")
docker export "$export_container" | zstd -T2 -8 -o "$out/$name"
(cd "$out" && sha256sum "$name" > "$name.sha256")
mkdir -p "$work/host-collector"
cp "$repo/dashboard/linux/linux_telemetry.py" "$repo/dashboard/"{system_events.py,storage_collect.py,storage_topology.py} "$repo/dashboard/lxc/"{install-host-collector.sh,proxmox_collect.py} "$work/host-collector/"
tar czf "$out/host-collector.tar.gz" -C "$work" host-collector
(cd "$out" && sha256sum host-collector.tar.gz > host-collector.tar.gz.sha256)
docker run --rm --entrypoint dpkg-query "$image" -W > "$out/packages-${commit:0:7}.txt"
printf 'source_commit=%s\nbase=%s\nbase_sha512=%s\ncaddy=%s\nimage=%s\n' "$commit" "$base" "$expected" "$caddy_image" "$image" > "$out/build-${commit:0:7}.txt"
echo "Built $out/$name"
