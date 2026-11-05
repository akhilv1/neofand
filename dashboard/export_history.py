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
from contextlib import closing
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
    destination.mkdir(parents=True,exist_ok=True)
    info=destination.stat()
    if info.st_uid not in (0,os.geteuid()) or info.st_mode & 0o022:
        raise PermissionError('Archive destination must be trusted and not group/world writable')
    output=destination/host
    if output.is_symlink():raise ValueError('Archive host directory must not be a symlink')
    output.mkdir(mode=0o700,exist_ok=True)
    info=output.stat()
    if info.st_uid!=os.geteuid() or info.st_mode & 0o077:
        raise PermissionError('Archive host directory must be owned by exporter with mode 0700')
    if mount_root:output.resolve().relative_to(root)
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    target=output/(stamp+'-'+uuid.uuid4().hex[:8]+'.tar.gz')
    with tempfile.TemporaryDirectory(prefix='neofand-export-') as temp:
        stage=Path(temp)/'snapshot';stage.mkdir();files=[]
        for name in SNAPSHOTS:
            nested=source/'telemetry'/name
            path=nested if name in ('linux.json','system-events.json','storage.json') and nested.exists() else source/name
            if path.exists():
                # Collectors atomically replace JSON; read a complete snapshot.
                payload=json.loads(path.read_text())
                (stage/name).write_text(json.dumps(payload));files.append(name)
        for index,path in enumerate(databases):
            path=Path(path).resolve(strict=True)
            name=f'{index}-{path.name}'
            with closing(sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)) as src,closing(sqlite3.connect(stage/name)) as dst:
                src.backup(dst,pages=256,sleep=.05)
                dst.execute('PRAGMA journal_mode=DELETE')
            files.append(name)
        if not files:raise RuntimeError('No data found to export')
        (stage/'manifest.json').write_text(json.dumps(dict(host=host,created_at=stamp,files=files),indent=2))
        archive=Path(temp)/'archive.tar.gz'
        with tarfile.open(archive,'w:gz') as tar:tar.add(stage,arcname=host)
        # Anchor writes/rename to the same opened directory, rejecting symlink races.
        directory=os.open(output,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        partial=target.name+'.partial'
        try:
            info=os.fstat(directory)
            if info.st_uid!=os.geteuid() or info.st_mode & 0o077:raise PermissionError('Untrusted archive directory')
            fd=os.open(partial,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=directory)
            with archive.open('rb') as src,os.fdopen(fd,'wb') as dst:
                shutil.copyfileobj(src,dst);dst.flush();os.fsync(dst.fileno())
            os.rename(partial,target.name,src_dir_fd=directory,dst_dir_fd=directory)
            os.fsync(directory)
        except BaseException:
            try:os.unlink(partial,dir_fd=directory)
            except FileNotFoundError:pass
            raise
        finally:os.close(directory)
    return target

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',required=True);p.add_argument('--destination',required=True)
    p.add_argument('--database',action='append',default=[]);p.add_argument('--host',default=socket.gethostname())
    p.add_argument('--mount-root',help='Require this mount to be present; prevents silently writing locally after a NAS unmount')
    a=p.parse_args();print(export(a.source,a.destination,a.database,a.host,a.mount_root))
