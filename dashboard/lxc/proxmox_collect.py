"""Read-only Proxmox inventory; root-local pvesh, no API token or guest controls."""
import json
import hashlib
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
import tempfile
import stat


def query(path,*args):
    r=subprocess.run(['pvesh','get',path,'--output-format','json',*args],capture_output=True,text=True,check=True,timeout=20)
    return json.loads(r.stdout)


def guest_storage(guests):
    """Publish only guest-to-storage relationships, never raw VM config or args."""
    uses=[]; seen=set()
    for guest in guests:
        vmid=guest['vmid']; kind=guest['type']
        if kind!='qemu':continue
        command=['qm' if kind=='qemu' else 'pct','config',str(vmid)]
        try:
            config=subprocess.run(command,capture_output=True,text=True,check=True,timeout=10).stdout
        except (OSError,subprocess.SubprocessError):
            continue
        sources=[]
        for line in config.splitlines():
            key,sep,value=line.partition(':')
            if not sep:continue
            if re.fullmatch(r'(?:scsi|sata|ide|virtio|efidisk|tpmstate|mp|rootfs)\d*',key):
                source=value.strip().split(',',1)[0]
                if source and source!='none' and not source.endswith('.iso'):sources.append(source)
            elif key=='args':
                sources.extend(re.findall(r'file=(/dev/[^,\s]+)',value))
        for source in dict.fromkeys(sources):
            if source.startswith('/dev/'):
                target=str(Path(source).resolve())
                # Only a complete physical disk conveys disk ownership.
                # Partitions, zvols and logical volumes must not claim their parent.
                try:
                    r=subprocess.run(['lsblk','-ndo','TYPE',target],capture_output=True,text=True,check=True,timeout=3)
                    if r.stdout.strip()!='disk':continue
                except (OSError,subprocess.SubprocessError):continue
                relation='passthrough'
            else:
                # Mounts and other virtual stores have no proven physical drive.
                continue
            identity=(target,vmid,relation)
            if identity not in seen:
                seen.add(identity)
                uses.append(dict(target=target,vmid=vmid,name=guest.get('name') or str(vmid),
                                 type=kind,status=guest.get('status'),relation=relation,whole_disk=True))
    return uses


def normalize_filesystems(items):
    """Use guest filesystem totals once, regardless of backing disk count."""
    if not isinstance(items,list):return []
    result=[];seen=set()
    for fs in items:
        mount=fs.get('mountpoint','');total=fs.get('total-bytes');used=fs.get('used-bytes')
        if not fs.get('disk') or not mount or re.match(r'^/(tmp|run|dev|proc|sys|boot)(/|$)',mount):continue
        if not isinstance(total,int) or not isinstance(used,int) or not 0<=used<=total or total<=0:continue
        identity=fs.get('name') or mount
        if identity in seen:continue
        seen.add(identity)
        result.append(dict(mountpoint=mount,filesystem=fs.get('type'),total_bytes=total,used_bytes=used))
    # DSM reports its mirrored OS partition too; show its actual data volumes.
    data=[fs for fs in result if re.fullmatch(r'/volume\d+',fs['mountpoint'])]
    return data or result


def guest_capacity(guests,uses):
    owners={u['vmid'] for u in uses if u.get('whole_disk')}
    result=[]
    for guest in guests:
        if guest['vmid'] not in owners or guest.get('status')!='running':continue
        entry=dict(vmid=guest['vmid'],name=guest.get('name'),status='unavailable',filesystems=[])
        try:
            r=subprocess.run(['qm','guest','cmd',str(guest['vmid']),'get-fsinfo'],capture_output=True,text=True,check=True,timeout=5)
            entry['filesystems']=normalize_filesystems(json.loads(r.stdout))
            entry['status']='ok' if entry['filesystems'] else 'unavailable'
        except (OSError,ValueError,TypeError,subprocess.SubprocessError):pass
        result.append(entry)
    return result


def normalize_smart(raw,exit_code=0):
    nvme=raw.get('nvme_smart_health_information_log',{})
    passed=raw.get('smart_status',{}).get('passed')
    sleeping=any(re.search(r'(standby|sleep) mode',m.get('string',''),re.I) for m in raw.get('smartctl',{}).get('messages',[]))
    attrs={a['id']:a for a in raw.get('ata_smart_attributes',{}).get('table',[])}
    value=lambda ident:attrs.get(ident,{}).get('raw',{}).get('value')
    available=isinstance(passed,bool) and not (exit_code & 7)
    state='sleeping' if sleeping else 'unavailable' if not available else 'failed' if not passed or nvme.get('critical_warning',0) else 'passed'
    history=[]
    if exit_code & 32:history.append('Past SMART threshold exceeded')
    if exit_code & 64:history.append('Recorded ATA errors')
    if exit_code & 128:history.append('Recorded self-test errors')
    if state=='passed' and (exit_code & 16 or history or nvme.get('media_errors',0)>0 or nvme.get('percentage_used',0)>=100 or any((value(i) or 0)>0 for i in (5,197,198))):state='warning'
    serial=raw.get('serial_number')
    return dict(state=state,model=raw.get('model_name'),device_id=hashlib.sha256((str(raw.get('model_name'))+':'+str(serial)).encode()).hexdigest()[:24] if serial else None,
                temperature_c=raw.get('temperature',{}).get('current'),endurance_used_pct=nvme.get('percentage_used'),
                available_spare_pct=nvme.get('available_spare'),media_errors=nvme.get('media_errors'),critical_warning=nvme.get('critical_warning'),
                power_on_hours=raw.get('power_on_time',{}).get('hours'),reallocated_sectors=value(5),pending_sectors=value(197),
                uncorrectable_sectors=value(198),ata_error_count=raw.get('ata_smart_error_log',{}).get('summary',{}).get('count'),
                history_warnings=history)


def storage_health(storage_path):
    try:storage=read_storage(storage_path)
    except (OSError,ValueError):return None,[]
    if time.time()-storage.get('timestamp',0)>180:return None,[]
    drives=[];cache={}
    for drive in storage.get('drives',[])[:128]:
        device=drive.get('device','')
        if not re.fullmatch(r'/dev/(?:nvme\d+n\d+|sd[a-z]+|hd[a-z]+)',device):continue
        if device in cache:continue
        try:
            r=subprocess.run(['smartctl','-a','-j','-n','standby,3',device],capture_output=True,text=True,timeout=5)
            smart=normalize_smart(json.loads(r.stdout),r.returncode)
        except (OSError,ValueError,subprocess.SubprocessError):smart={'state':'unavailable'}
        smart['model']=smart.get('model') or drive.get('model') or device
        cache[device]=smart;drives.append(dict(device=device,smart=smart))
    pools=[]
    for pool in storage.get('pools',[]):
        members=[dict(device=dev,partition=m.get('path'),state=m.get('state'),smart=cache.get(dev,{'state':'unavailable'}))
                 for m in pool.get('members',[]) for dev in m.get('backing_drives',[])]
        pools.append(dict(name=pool.get('name'),health=pool.get('health'),topology=pool.get('topology',[]),
                          scan=pool.get('scan'),timestamp=storage.get('timestamp'),members=members))
    return drives,pools


HEALTH_FIELDS=('state','critical_warning','media_errors','reallocated_sectors','pending_sectors','uncorrectable_sectors','ata_error_count','endurance_used_pct','available_spare_pct','history_warnings')

def health_history(previous,drives,now,retention_days=365):
    """Durable transitions; standby is neutral and failed reads need three samples."""
    states={k:dict(v) for k,v in previous.get('states',{}).items()};events=list(previous.get('events',[]));seen=set()
    def event(device,title,message,severity):
        ident=hashlib.sha256(f'{now}:{device}:{title}:{message}'.encode()).hexdigest()[:24]
        events.append(dict(id=ident,ts=now,source='SMART',kind='storage',title=title,unit=device,message=message,severity=severity,confidence='recorded'))
    for item in drives:
        device=item['device'];smart=item['smart'];key=smart.get('device_id') or next((k for k,v in states.items() if v.get('device')==device),device)
        if key!=device and key not in states and device in states and not states[device].get('health'):
            states[key]=states.pop(device)
        seen.add(key);old=states.get(key,{});current=smart.get('state','unavailable')
        if current=='sleeping':
            states[key]={**old,'device':device,'misses':0,'last_seen':now};continue
        if current=='unavailable':
            misses=old.get('misses',0)+1
            if misses==3:event(device,'Drive health readings unavailable','SMART could not be read for three consecutive samples.','warning')
            states[key]={**old,'device':device,'misses':misses,'last_seen':now};continue
        if old.get('misses',0)>=3:event(device,'Drive health readings restored','SMART readings are available again.','info')
        before=old.get('health');after={k:smart.get(k) for k in HEALTH_FIELDS}
        if before is None:
            if current in ('warning','failed'):
                event(device,'Existing drive health warning observed',f'{smart.get("model",device)}: {current}; '+('; '.join(smart.get('history_warnings',[])) or 'SMART health indicators require attention')+'. Initial baseline; this is not a newly detected deterioration.','error' if current=='failed' else 'warning')
        else:
            changes=[f'{k.replace("_"," ")}: {before.get(k)} → {after[k]}' for k in HEALTH_FIELDS if before.get(k)!=after[k]]
            if changes:
                danger=any(isinstance(after.get(k),(int,float)) and after[k]>(before.get(k) or 0) for k in ('media_errors','pending_sectors','reallocated_sectors','uncorrectable_sectors','ata_error_count'))
                severity='error' if current=='failed' else 'warning' if current=='warning' or danger else 'info'
                event(device,'Drive health changed',f'{smart.get("model",device)} · '+'; '.join(changes),severity)
        states[key]=dict(device=device,health=after,misses=0,last_seen=now)
    for key,old in list(states.items()):
        if key in seen:continue
        misses=old.get('misses',0)+1
        if misses==3:event(old['device'],'Drive no longer detected','Physical drive is absent from three consecutive storage snapshots.','warning')
        old['misses']=misses
        if now-old.get('last_seen',now)>retention_days*86400:del states[key]
    cutoff=now-retention_days*86400
    events=sorted((e for e in events if e['ts']>=cutoff),key=lambda e:e['ts'],reverse=True)[:10000]
    return dict(timestamp=now,status='ok',retention_days=retention_days,states=states,events=events)


def atomic_json(path,data):
    """Only publish in a trusted root-owned directory; never follow temp symlinks."""
    path=Path(path)
    parent=path.parent.stat()
    if path.parent.is_symlink() or parent.st_uid!=os.geteuid() or parent.st_mode & 0o022:
        raise PermissionError('Snapshot directory must be owned by this service and not group/world writable')
    if path.is_symlink():raise PermissionError('Refusing symlink snapshot')
    fd,temp=tempfile.mkstemp(prefix='.'+path.name+'.',dir=path.parent)
    try:
        with os.fdopen(fd,'w') as f:
            os.fchmod(f.fileno(),0o644)
            json.dump(data,f);f.flush();os.fsync(f.fileno())
        os.replace(temp,path)
        directory=os.open(path.parent,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:os.fsync(directory)
        finally:os.close(directory)
    finally:
        if os.path.exists(temp):os.unlink(temp)


def read_storage(path):
    # This input is produced by a less-privileged sampler: bound it and reject links.
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    with os.fdopen(fd) as f:
        info=os.fstat(f.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size>4*1024*1024:raise ValueError('Invalid storage snapshot')
        return json.loads(f.read(4*1024*1024+1))


def save_health_history(path,drives):
    if path.is_symlink():raise PermissionError('Refusing symlink health history')
    try:previous=json.loads(path.read_text())
    except FileNotFoundError:previous={}
    # Do not discard a corrupt baseline silently or repeatedly seed false alerts.
    data=health_history(previous,drives,time.time(),max(1,min(3650,int(os.environ.get('NEOFAND_DRIVE_EVENT_RETENTION_DAYS','365')))))
    atomic_json(path,data)


def normalize(node,status,resources,tasks):
    guests=[]
    for item in resources:
        if item.get('node')!=node or item.get('type') not in ('qemu','lxc'):continue
        guests.append({key:item.get(key) for key in ('vmid','name','type','status','cpu','maxcpu','mem','maxmem','disk','maxdisk','uptime')})
    cached_node=next((r for r in resources if r.get('type')=='node' and (r.get('node')==node or r.get('id')=='node/'+node)),{})
    # pvesh's one-shot node/status CPU sample can be an uninitialized zero.
    host={k:status.get(k) for k in ('pveversion','uptime','loadavg','memory','swap','rootfs')}
    host['cpu']=cached_node.get('cpu')
    recent=[{key:t.get(key) for key in ('type','id','starttime','endtime','status')} for t in tasks[:50]]
    return dict(timestamp=time.time(),status='ok',node=node,
                host=host,
                guests=sorted(guests,key=lambda g:int(g['vmid'])),tasks=recent)


def main(path):
    target=Path(path);node=socket.gethostname().split('.')[0]
    if not re.fullmatch(r'[A-Za-z0-9._-]+',node):raise ValueError('Invalid node name')
    try:
        data=normalize(node,query('/nodes/'+node+'/status'),query('/cluster/resources'),query('/nodes/'+node+'/tasks','--limit','50'))
        data['storage_uses']=guest_storage(data['guests'])
        data['guest_storage']=guest_capacity(data['guests'],data['storage_uses'])
        storage=target.parent/'telemetry/storage.json'
        drives,data['pool_health']=storage_health(storage if storage.exists() else target.parent/'storage.json')
        data['drive_health']=drives or []
        if drives is not None:save_health_history(target.parent/'drive-health-history.json',drives)
    except (OSError,ValueError,KeyError,subprocess.SubprocessError):
        try:data=json.loads(target.read_text())
        except (OSError,ValueError):data={'timestamp':0,'guests':[],'tasks':[],'node':node}
        data.update(status='unavailable',attempted_at=time.time())
    target.parent.mkdir(parents=True,exist_ok=True)
    atomic_json(target,data)

if __name__=='__main__':main(sys.argv[1])
