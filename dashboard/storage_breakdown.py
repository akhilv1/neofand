"""Low-priority metadata-only storage categorization. Never follows symlinks."""
from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import re
import stat
import sys
import time

CATEGORIES = ('Models', 'Photos', 'Videos', 'Audio', 'Backups', 'Archives',
              'Apps & Developer', 'Containers', 'Caches & Logs', 'Documents', 'System', 'Other')
MODEL_EXT = {'.gguf', '.ggml', '.safetensors', '.onnx', '.pt', '.pth', '.ckpt', '.tflite', '.mlmodel', '.h5', '.hdf5'}
PHOTO_EXT = {'.jpg', '.jpeg', '.png', '.heic', '.heif', '.gif', '.webp', '.tif', '.tiff', '.raw', '.cr2', '.cr3', '.nef', '.arw', '.dng', '.avif'}
VIDEO_EXT = {'.mp4', '.mov', '.mkv', '.avi', '.m4v', '.webm', '.mts', '.mpeg', '.mpg'}
AUDIO_EXT = {'.mp3', '.flac', '.wav', '.aiff', '.aac', '.m4a', '.ogg', '.opus'}
ARCHIVE_EXT = {'.zip', '.tar', '.gz', '.xz', '.7z', '.rar', '.zst', '.bz2', '.iso', '.dmg'}
DOC_EXT = {'.pdf', '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx', '.txt', '.md', '.csv', '.epub', '.pages', '.numbers'}
CODE_EXT = {'.py', '.rs', '.js', '.ts', '.tsx', '.jsx', '.c', '.h', '.cpp', '.go', '.java', '.so', '.a', '.o', '.deb', '.rpm', '.whl'}


def category(path):
    parts = tuple(p.lower() for p in Path(path).parts)
    names = set(parts)
    suffix = Path(path).suffix.lower()
    if any(p in {'backup', 'backups', 'backups.backupdb', '.timemachine', 'time machine backups'} or p.endswith('.backupbundle') or p.endswith('.sparsebundle') for p in parts) or suffix in {'.bak', '.backup'}:
        return 'Backups'
    if names & {'models', 'model', 'huggingface', '.ollama', 'checkpoints'} or suffix in MODEL_EXT:
        return 'Models'
    if parts[:4] in {('/', 'var', 'lib', 'docker'), ('/', 'var', 'lib', 'containerd')}:
        return 'Containers'
    if names & {'.cache', 'cache', 'caches', 'logs', '.npm', '.triton'} or suffix == '.log':
        return 'Caches & Logs'
    if names & {'.git', 'node_modules', '.venv', '.venvs', 'venv', '.cargo', 'site-packages'}:
        return 'Apps & Developer'
    if len(parts)>1 and parts[1] in {'usr', 'boot', 'etc', 'lib', 'lib64', 'bin', 'sbin'}:
        return 'System'
    if suffix in PHOTO_EXT or any(p.endswith('.photoslibrary') for p in parts): return 'Photos'
    if suffix in VIDEO_EXT: return 'Videos'
    if suffix in AUDIO_EXT: return 'Audio'
    if suffix in ARCHIVE_EXT: return 'Archives'
    if suffix in DOC_EXT: return 'Documents'
    if suffix in CODE_EXT or (len(parts)>1 and parts[1]=='opt'): return 'Apps & Developer'
    return 'Other'


def mount_paths():
    result=set()
    for line in Path('/proc/1/mountinfo').read_text().splitlines():
        result.add(re.sub(r'\\([0-7]{3})', lambda m: chr(int(m.group(1),8)), line.split()[4]))
    return result


def folder_bucket(path, mount):
    relative=Path(path).relative_to(mount)
    # Group by directories only: no individual filenames are exported.
    parts=relative.parent.parts
    depth=3 if mount=='/' else 2
    return str(Path(mount).joinpath(*parts[:depth])) if parts else mount+' (top-level files)'


def scan(mount, filesystem_id, boundaries, deadline):
    started=int(time.time()); device=os.stat(mount).st_dev
    if f'{os.major(device)}:{os.minor(device)}' != filesystem_id:
        raise OSError('Mount identity changed')
    counts=Counter(); folders=Counter(); per_category=defaultdict(Counter)
    hardlinks=set(); pending=[mount]; errors=0; file_count=0; complete=True
    while pending:
        directory=pending.pop()
        if time.monotonic()>deadline:
            complete=False; break
        try:
            with os.scandir(directory) as entries:
                for entry in entries:
                    if time.monotonic()>deadline: complete=False; break
                    if entry.path in boundaries and entry.path!=mount: continue
                    try:
                        st=entry.stat(follow_symlinks=False)
                        if st.st_dev!=device: continue
                        is_directory=stat.S_ISDIR(st.st_mode)
                        if is_directory: pending.append(entry.path)
                        if not (is_directory or stat.S_ISREG(st.st_mode) or stat.S_ISLNK(st.st_mode)): continue
                        if not is_directory and st.st_nlink>1:
                            key=(st.st_dev,st.st_ino)
                            if key in hardlinks: continue
                            hardlinks.add(key)
                        size=st.st_blocks*512
                        kind=category(entry.path)
                        folder=folder_bucket(entry.path,mount)
                        counts[kind]+=size;folders[folder]+=size;per_category[kind][folder]+=size
                        if not is_directory:file_count+=1
                    except OSError: errors+=1
        except OSError: errors+=1
        if not complete: break
    def top(counter):return [{'path':p,'bytes':b} for p,b in counter.most_common(8) if b]
    return dict(filesystem_id=filesystem_id,mount=mount,started_at=started,timestamp=int(time.time()),
                complete=complete,unreadable_entries=errors,files_scanned=file_count,
                scanned_bytes=sum(counts.values()),
                categories=[dict(name=n,bytes=counts[n],folders=top(per_category[n])) for n in CATEGORIES],
                largest_folders=top(folders))


def collect(snapshot):
    boundaries=mount_paths(); seen=set(); results=[]
    def walk(n):
        yield n
        for child in n.get('children',[]):yield from walk(child)
    deadline=time.monotonic()+1100
    for drive in snapshot['drives']:
        for n in walk(drive):
            fsid=n['filesystem_id']
            if not n.get('usage') or not n.get('mounts') or fsid in seen:continue
            seen.add(fsid)
            mount=min(n['mounts'],key=len)
            try: results.append(scan(mount,fsid,boundaries,deadline))
            except OSError:results.append(dict(filesystem_id=fsid,mount=mount,error='Filesystem unavailable'))
    return dict(timestamp=int(time.time()),filesystems=results,method='allocated blocks; path and extension classification')


if __name__=='__main__':
    source,destination=map(Path,sys.argv[1:3])
    data=collect(json.loads(source.read_text()))
    temp=destination.with_suffix('.tmp');temp.write_text(json.dumps(data,allow_nan=False));temp.chmod(0o644);temp.replace(destination)
