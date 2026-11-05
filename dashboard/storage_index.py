"""Persistent, resumable metadata index. Cached classifications; periodic reconciliation."""
from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import sqlite3
import stat
import sys
import time
from storage_breakdown import category, folder_bucket, mount_paths


def atomic(path, data):
    path=Path(path);temp=path.with_suffix('.tmp');temp.write_text(json.dumps(data,allow_nan=False));temp.chmod(0o644);temp.replace(path)


class Index:
    def __init__(self,path):
        self.db=sqlite3.connect(path)
        self.db.execute('PRAGMA journal_mode=WAL');self.db.execute('PRAGMA synchronous=NORMAL')
        self.db.executescript('''
CREATE TABLE IF NOT EXISTS runs (fs TEXT PRIMARY KEY, mount TEXT, generation INTEGER, state TEXT, started INTEGER, completed INTEGER, files INTEGER, dirs INTEGER, errors INTEGER, reused INTEGER);
CREATE TABLE IF NOT EXISTS directories (fs TEXT, path TEXT, generation INTEGER, entries TEXT, PRIMARY KEY(fs,path));
CREATE TABLE IF NOT EXISTS pending (fs TEXT, path TEXT, PRIMARY KEY(fs,path));
''')
    def start(self,fs,mount,force=False):
        old=self.db.execute('SELECT generation,state,completed,mount FROM runs WHERE fs=?',(fs,)).fetchone()
        if old and old[1]=='indexing' and old[3]==mount:return True
        if old and old[1]=='ready' and time.time()-(old[2] or 0)<1800 and not force:return False
        generation=(old[0]+1) if old else 1
        self.db.execute('DELETE FROM pending WHERE fs=?',(fs,))
        self.db.execute('INSERT INTO pending VALUES (?,?)',(fs,mount))
        self.db.execute('INSERT OR REPLACE INTO runs VALUES (?,?,?,?,?,?,?,?,?,?)',(fs,mount,generation,'indexing',int(time.time()),old[2] if old else None,0,0,0,0));self.db.commit()
        return True
    def process_directory(self,fs,boundaries):
        run=self.db.execute('SELECT mount,generation FROM runs WHERE fs=?',(fs,)).fetchone();mount,generation=run
        todo=self.db.execute('SELECT path FROM pending WHERE fs=? LIMIT 1',(fs,)).fetchone()
        if not todo:return False
        directory=todo[0];prior=self.db.execute('SELECT entries FROM directories WHERE fs=? AND path=?',(fs,directory)).fetchone()
        cache=json.loads(prior[0]) if prior else {};current={};children=[];files=errors=reused=0
        try:
            root=os.lstat(directory)
            if not stat.S_ISDIR(root.st_mode) or f'{os.major(root.st_dev)}:{os.minor(root.st_dev)}'!=fs:raise OSError('Mount changed')
            with os.scandir(directory) as entries:
                for e in entries:
                    if e.path in boundaries and e.path!=mount:continue
                    try:
                        st=e.stat(follow_symlinks=False)
                        if st.st_dev!=root.st_dev:continue
                        mode=stat.S_IFMT(st.st_mode)
                        if mode not in (stat.S_IFDIR,stat.S_IFREG,stat.S_IFLNK):continue
                        if mode==stat.S_IFDIR:children.append(e.path)
                        else:files+=1
                        fingerprint=[st.st_ino,mode,st.st_nlink,st.st_size,st.st_mtime_ns,st.st_ctime_ns,st.st_blocks*512]
                        old=cache.get(e.name)
                        if old and old[:7]==fingerprint:
                            current[e.name]=old;reused+=1
                        else:current[e.name]=fingerprint+[category(e.path),folder_bucket(e.path,mount)]
                    except OSError:errors+=1
        except OSError:errors+=1
        with self.db:
            self.db.execute('INSERT OR REPLACE INTO directories VALUES (?,?,?,?)',(fs,directory,generation,json.dumps(current,separators=(',',':'))))
            self.db.execute('DELETE FROM pending WHERE fs=? AND path=?',(fs,directory))
            self.db.executemany('INSERT OR IGNORE INTO pending VALUES (?,?)',[(fs,c) for c in children])
            self.db.execute('UPDATE runs SET files=files+?,dirs=dirs+1,errors=errors+?,reused=reused+? WHERE fs=?',(files,errors,reused,fs))
        return True
    def finish(self,fs):
        mount,generation,started,files,errors=self.db.execute('SELECT mount,generation,started,files,errors FROM runs WHERE fs=?',(fs,)).fetchone()
        counts=Counter();folders=Counter();groups=defaultdict(Counter);hardlinks=set()
        for (blob,) in self.db.execute('SELECT entries FROM directories WHERE fs=? AND generation=?',(fs,generation)):
            for item in json.loads(blob).values():
                inode,mode,nlink,_,_,_,size,kind,folder=item
                if mode!=stat.S_IFDIR and nlink>1:
                    if inode in hardlinks:continue
                    hardlinks.add(inode)
                counts[kind]+=size;folders[folder]+=size;groups[kind][folder]+=size
        now=int(time.time())
        with self.db:
            self.db.execute('DELETE FROM directories WHERE fs=? AND generation!=?',(fs,generation))
            self.db.execute("UPDATE runs SET state='ready',completed=? WHERE fs=?",(now,fs))
        def top(c):return [dict(path=p,bytes=b) for p,b in c.most_common(8) if b]
        return dict(filesystem_id=fs,mount=mount,started_at=started,timestamp=now,complete=True,
                    unreadable_entries=errors,files_scanned=files,scanned_bytes=sum(counts.values()),
                    categories=[dict(name=k,bytes=b,folders=top(groups[k])) for k,b in counts.items()],largest_folders=top(folders))
    def progress(self,targets,paused=False):
        rows=[]
        for fs in targets:
            r=self.db.execute('SELECT mount,state,started,completed,files,dirs,errors,reused FROM runs WHERE fs=?',(fs,)).fetchone()
            if not r:rows.append(dict(filesystem_id=fs,state='queued'));continue
            mount,state,started,completed,files,dirs,errors,reused=r
            pending=self.db.execute('SELECT COUNT(*) FROM pending WHERE fs=?',(fs,)).fetchone()[0]
            rows.append(dict(filesystem_id=fs,mount=mount,state='paused' if paused and state=='indexing' else state,started_at=started,indexed_at=completed,files_checked=files,folders_checked=dirs,folders_pending=pending,unreadable_entries=errors,cached_entries_reused=reused))
        return dict(timestamp=int(time.time()),filesystems=rows,method='Persistent metadata cache with periodic reconciliation')


def run(source,destination,deadline_seconds=1100):
    source=Path(source);destination=Path(destination);directory=destination.parent
    snapshot=json.loads(source.read_text());targets={n['filesystem_id']:n for n in snapshot.get('scan_targets',[]) if n.get('usage') and n.get('mounts')}
    if 'scan_targets' not in snapshot:
        from storage_topology import walk
        targets={n['filesystem_id']:n for d in snapshot['drives'] for n in walk(d) if n.get('usage') and n.get('mounts') and not n.get('auxiliary')}
    index=Index(directory/'storage-index.sqlite3');progress_path=directory/'storage-index-progress.json'
    try:result=json.loads(destination.read_text())
    except (OSError,ValueError):result={'filesystems':[]}
    previous={n['filesystem_id']:n for n in result['filesystems']};boundaries=mount_paths();deadline=time.monotonic()+deadline_seconds;last_update=0
    for fs,n in targets.items():
        mount=min(n['mounts'],key=len)
        try:
            device=os.stat(mount).st_dev
            if f'{os.major(device)}:{os.minor(device)}'!=fs:continue
        except OSError:continue
        if not index.start(fs,mount):continue
        atomic(progress_path,index.progress(targets))
        while time.monotonic()<deadline and index.process_directory(fs,boundaries):
            if time.monotonic()-last_update>2:
                atomic(progress_path,index.progress(targets));last_update=time.monotonic()
        if index.db.execute('SELECT COUNT(*) FROM pending WHERE fs=?',(fs,)).fetchone()[0]:break
        previous[fs]=index.finish(fs)
        # Publish each finished filesystem immediately; others retain their last cached breakdown.
        atomic(destination,dict(timestamp=int(time.time()),filesystems=[v for k,v in previous.items() if k in targets],method='persistent metadata index; allocated blocks'))
        atomic(progress_path,index.progress(targets))
    atomic(progress_path,index.progress(targets,paused=time.monotonic()>=deadline))
    index.db.close()


if __name__=='__main__':run(*sys.argv[1:3])
