"""Read-only physical drive inventory and mounted filesystem usage; run on host."""
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import time
from storage_topology import parse_mountinfo, discover_zfs, enrich


def collect():
    result = subprocess.run(['lsblk', '--json', '--bytes', '--exclude', '7', '--output',
                             'NAME,KNAME,TYPE,SIZE,MODEL,FSTYPE,PARTTYPE,PARTTYPENAME,MOUNTPOINTS,MAJ:MIN,ROTA,TRAN'],
                            check=True, capture_output=True, text=True, timeout=10)
    devices = json.loads(result.stdout)['blockdevices']
    # systemd hardening creates private bind mounts; use PID 1's host mount table.
    mounts = parse_mountinfo(Path('/proc/1/mountinfo').read_text())
    host_mounts = {}
    for mount in mounts:
        host_mounts.setdefault(mount['id'], []).append(mount['path'])

    def describe(node):
        mounts = list(dict.fromkeys(host_mounts.get(node.get('maj:min'), [])))
        item = {key: node.get(key) for key in ('name', 'kname', 'type', 'size', 'model', 'fstype', 'rota', 'tran', 'parttype', 'parttypename')}
        partition_type = (node.get('parttypename') or '').lower()
        item['auxiliary'] = partition_type in {'efi system', 'bios boot', 'microsoft reserved', 'apple boot', 'apple label'} or 'metadata' in partition_type
        item.update(device='/dev/' + node['name'], filesystem_id=node.get('maj:min'),
                    mounts=mounts, usage=None, usage_status='unmounted')
        for mount in mounts:
            try:
                dev = os.stat(mount).st_dev
                if f'{os.major(dev)}:{os.minor(dev)}' != node.get('maj:min'):
                    continue  # Mount changed since inventory; never measure its parent filesystem.
                st = os.statvfs(mount)
                total, free, available = (v * st.f_frsize for v in (st.f_blocks, st.f_bfree, st.f_bavail))
                item['usage'] = dict(total_bytes=total, used_bytes=total-free,
                                     available_bytes=available, reserved_bytes=max(0, free-available))
                item['usage_status'] = 'mounted'
                break
            except OSError:
                pass
        if mounts and item['usage'] is None:
            item['usage_status'] = 'unavailable'
        item['children'] = [describe(child) for child in node.get('children', [])]
        return item

    physical = [d for d in devices if d['type']=='disk' and not (d.get('kname') or d['name']).startswith('zd')]
    snapshot = {'timestamp': int(time.time()), 'drives': [describe(d) for d in physical]}
    snapshot['unbacked_nodes'] = [describe(d) for d in devices if d['type'] not in {'disk','loop'}]
    pools, zfs_status = discover_zfs(mounts)
    return enrich(snapshot, mounts, pools, zfs_status)


def main():
    destination = Path(sys.argv[1])
    data = collect()
    temporary = destination.with_suffix('.tmp')
    temporary.write_text(json.dumps(data, allow_nan=False))
    temporary.chmod(0o644)
    temporary.replace(destination)


if __name__ == '__main__':
    main()
