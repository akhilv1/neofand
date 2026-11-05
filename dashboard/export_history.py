#!/usr/bin/env python3
"""Export per-host snapshots; live SQLite databases remain on local storage."""
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import socket
import sqlite3
import tarfile
import tempfile
import uuid

SNAPSHOTS=('linux.json','system-events.json','storage.json','proxmox.json','drive-health-history.json')

def export(source,destination,databases,host,mount_root=None):
    source=Path(source).resolve();destination=Path(destination).resolve()
    if not re.fullmatch(r'[A-Za-z0-9._-]+',host) or host in ('.','..'):raise ValueError('Invalid host identifier')
    if mount_root:
        root=Path(mount_root).resolve()
        if not os.path.ismount(root):raise RuntimeError('Archive mount is not mounted')
        destination.relative_to(root)
    output=destination/host;output.mkdir(parents=True,exist_ok=True)
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    target=output/(stamp+'-'+uuid.uuid4().hex[:8]+'.tar.gz')
    with tempfile.TemporaryDirectory(prefix='neofand-export-') as temp:
        stage=Path(temp)/'snapshot';stage.mkdir();files=[]
        for name in SNAPSHOTS:
            path=source/name
            if path.exists():
                # Collectors atomically replace JSON; read a complete snapshot.
                payload=json.loads(path.read_text())
                (stage/name).write_text(json.dumps(payload));files.append(name)
        for index,path in enumerate(databases):
            path=Path(path).resolve(strict=True)
            name=f'{index}-{path.name}'
            with sqlite3.connect(path.as_uri()+'?mode=ro',uri=True) as src,sqlite3.connect(stage/name) as dst:
                src.backup(dst,pages=256,sleep=.05)
                dst.execute('PRAGMA journal_mode=DELETE')
            files.append(name)
        if not files:raise RuntimeError('No data found to export')
        (stage/'manifest.json').write_text(json.dumps(dict(host=host,created_at=stamp,files=files),indent=2))
        archive=Path(temp)/'archive.tar.gz'
        with tarfile.open(archive,'w:gz') as tar:tar.add(stage,arcname=host)
        partial=target.with_suffix('.partial')
        try:
            with archive.open('rb') as src,partial.open('xb') as dst:
                shutil.copyfileobj(src,dst);dst.flush();os.fsync(dst.fileno())
            partial.replace(target)
        except BaseException:
            partial.unlink(missing_ok=True);raise
    return target

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',required=True);p.add_argument('--destination',required=True)
    p.add_argument('--database',action='append',default=[]);p.add_argument('--host',default=socket.gethostname())
    p.add_argument('--mount-root',help='Require this mount to be present; prevents silently writing locally after a NAS unmount')
    a=p.parse_args();print(export(a.source,a.destination,a.database,a.host,a.mount_root))
