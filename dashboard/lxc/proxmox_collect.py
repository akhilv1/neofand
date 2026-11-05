"""Read-only Proxmox inventory; root-local pvesh, no API token or guest controls."""
import json
from pathlib import Path
import re
import socket
import subprocess
import sys
import time


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
    except (OSError,ValueError,KeyError,subprocess.SubprocessError):
        try:data=json.loads(target.read_text())
        except (OSError,ValueError):data={'timestamp':0,'guests':[],'tasks':[],'node':node}
        data.update(status='unavailable',attempted_at=time.time())
    target.parent.mkdir(parents=True,exist_ok=True)
    tmp=target.with_suffix('.tmp');tmp.write_text(json.dumps(data));tmp.chmod(0o644);tmp.replace(target)

if __name__=='__main__':main(sys.argv[1])
