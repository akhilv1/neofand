"""Read-only physical drive inventory and mounted filesystem usage; run on host."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def collect():
    result = subprocess.run(['lsblk', '--json', '--bytes', '--exclude', '7', '--output',
                             'NAME,TYPE,SIZE,MODEL,FSTYPE,MOUNTPOINTS,MAJ:MIN,ROTA,TRAN'],
                            check=True, capture_output=True, text=True, timeout=10)
    devices = json.loads(result.stdout)['blockdevices']

    def describe(node):
        mounts = [m for m in node.get('mountpoints', []) if m and m.startswith('/')]
        item = {key: node.get(key) for key in ('name', 'type', 'size', 'model', 'fstype', 'rota', 'tran')}
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

    return {'timestamp': int(time.time()), 'drives': [describe(d) for d in devices if d['type'] == 'disk']}


def main():
    destination = Path(sys.argv[1])
    data = collect()
    temporary = destination.with_suffix('.tmp')
    temporary.write_text(json.dumps(data, allow_nan=False))
    temporary.chmod(0o644)
    temporary.replace(destination)


if __name__ == '__main__':
    main()
