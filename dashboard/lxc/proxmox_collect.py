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
    except (OSError,ValueError,KeyError,subprocess.SubprocessError):
        try:data=json.loads(target.read_text())
        except (OSError,ValueError):data={'timestamp':0,'guests':[],'tasks':[],'node':node}
        data.update(status='unavailable',attempted_at=time.time())
    target.parent.mkdir(parents=True,exist_ok=True)
    tmp=target.with_suffix('.tmp');tmp.write_text(json.dumps(data));tmp.chmod(0o644);tmp.replace(target)

if __name__=='__main__':main(sys.argv[1])
