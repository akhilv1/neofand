"""Read-only Linux storage graph and optional OpenZFS discovery."""
from collections import defaultdict
import os
from pathlib import Path
import re
import shutil
import subprocess


def walk(node):
    yield node
    for child in node.get('children', []):
        yield from walk(child)


def roles(mounts):
    return (['System'] if '/' in mounts else []) + (['Boot'] if any(m in {'/boot', '/boot/efi', '/efi'} for m in mounts) else [])


def parse_mountinfo(text):
    records=[]
    decode=lambda value: re.sub(r'\\([0-7]{3})',lambda m:chr(int(m.group(1),8)),value)
    for line in text.splitlines():
        left,right=line.split(' - ',1); fields=left.split(); tail=right.split()
        records.append(dict(id=fields[2],path=decode(fields[4]),fstype=tail[0],source=decode(tail[1])))
    return records


def parse_zpool_status(text):
    """Parse the stable C-locale indented config tree, preserving auxiliary vdevs."""
    result={}; pool=None; in_config=False; stack=[]
    states='ONLINE|DEGRADED|FAULTED|OFFLINE|UNAVAIL|REMOVED|AVAIL|INUSE|SUSPENDED'
    for original in text.splitlines():
        line=original.expandtabs(8); stripped=line.strip()
        if stripped.startswith('pool:'):
            pool=stripped.split(':',1)[1].strip();result[pool]={'topology':[],'scan':None};in_config=False;stack=[]
        elif pool and stripped.startswith('scan:'):result[pool]['scan']=stripped.split(':',1)[1].strip()
        elif stripped=='config:':in_config=True
        elif stripped.startswith('errors:'):in_config=False
        elif pool and in_config and stripped and not stripped.startswith('NAME'):
            match=re.match(r'^(.*?)\s+('+states+r')(?:\s+|$)',stripped)
            if match:name,state=match.group(1),match.group(2)
            elif stripped in {'logs','cache','spares','special','dedup'}:name,state=stripped,None
            else:continue
            indent=len(line)-len(line.lstrip());node={'name':name,'state':state,'children':[]}
            while stack and stack[-1][0]>=indent:stack.pop()
            if stack:stack[-1][1]['children'].append(node)
            else:result[pool]['topology'].append(node)
            stack.append((indent,node))
    return result


def number(value):
    try:return int(value)
    except (ValueError,TypeError):return None


def parse_zfs_list(text, mounts):
    datasets=[]
    for line in text.splitlines():
        f=line.split('\t')
        if len(f)!=9:continue
        name,kind,used,available,referenced,own,snaps,mountpoint,mounted=f
        actual=[m for m in mounts if m['fstype']=='zfs' and m['source']==name]
        datasets.append(dict(name=name,type=kind,used_bytes=number(used),available_bytes=number(available),
                             referenced_bytes=number(referenced),own_used_bytes=number(own),snapshot_used_bytes=number(snaps),
                             mountpoint=mountpoint,mounts=[m['path'] for m in actual],
                             filesystem_id=actual[0]['id'] if actual else None,roles=roles([m['path'] for m in actual])))
    return datasets


def discover_zfs(mounts, runner=None):
    if not shutil.which('zpool') or not shutil.which('zfs'):
        return [],'not-installed'
    def run(args):
        return subprocess.run(args,check=True,capture_output=True,text=True,timeout=3,env={**os.environ,'LC_ALL':'C'}).stdout
    run=runner or run
    try:
        pools_text=run(['zpool','list','-Hp','-o','name,size,alloc,free,health'])
        status=parse_zpool_status(run(['zpool','status','-LP']))
        datasets=parse_zfs_list(run(['zfs','list','-Hp','-t','filesystem,volume','-o','name,type,used,available,referenced,usedbydataset,usedbysnapshots,mountpoint,mounted']),mounts)
    except (OSError,subprocess.SubprocessError):return [],'unavailable'
    pools=[]
    for line in pools_text.splitlines():
        f=line.split('\t')
        if len(f)!=5:continue
        name,size,allocated,free,health=f
        ds=[d for d in datasets if d['name'].split('/')[0]==name]
        root=next((d for d in ds if d['name']==name),None)
        pools.append(dict(id='zfs:'+name,name=name,kind='ZFS',health=health,physical_size_bytes=number(size),
                          allocated_bytes=number(allocated),free_bytes=number(free),datasets=ds,
                          accounting={'used_bytes':root['used_bytes'],'available_bytes':root['available_bytes']} if root else None,
                          **status.get(name,{'topology':[],'scan':None})))
    return pools,'available'


def md_health(node):
    path=Path('/sys/class/block')/node.get('kname',node['name'])/'md'
    values={}
    for name in ('level','array_state','degraded','raid_disks','sync_action'):
        try:values[name]=(path/name).read_text().strip()
        except OSError:pass
    return values


def enrich(snapshot, mounts, pools, zfs_status):
    """Collapse repeated lsblk nodes into a graph; propagate roles to backing drives."""
    records={}; backing=defaultdict(set); parents=defaultdict(set)
    def visit(node, drive, parent=None):
        ident=node['filesystem_id']
        if ident not in records:records[ident]=dict(node)
        if drive:backing[ident].add(drive)
        if parent:parents[ident].add(parent)
        for child in node.get('children',[]):visit(child,drive,ident)
    for drive in snapshot['drives']:visit(drive,drive['device'])
    for node in snapshot.pop('unbacked_nodes',[]):visit(node,None)
    logical_ids={i for i,n in records.items() if n['type'] not in {'disk','part'}}
    changed=True
    while changed:
        before=len(logical_ids)
        logical_ids.update(i for i in records if parents[i] & logical_ids)
        changed=len(logical_ids)!=before
    for ident,node in records.items():
        node['roles']=roles(node['mounts']);node['backing_drives']=sorted(backing[ident]);node['parent_ids']=sorted(parents[ident])
        node['logical']=ident in logical_ids
        node['child_ids']=sorted({c['filesystem_id'] for c in node.get('children',[])})
        if node['type'].startswith('raid') or node['type']=='md':node['raid']=md_health(node)
    logical={i:n for i,n in records.items() if n['logical']}
    path_map={os.path.realpath(n['device']):i for i,n in records.items()}
    for pool in pools:
        members=[]
        def vdevs(nodes,role='data'):
            for node in nodes:
                kind=node['name'] if node['name'] in {'logs','cache','spares','special','dedup'} else role
                if node['name'].startswith('/'):
                    key=path_map.get(os.path.realpath(node['name']))
                    members.append(dict(path=node['name'],state=node['state'],role=kind,filesystem_id=key,
                                        backing_drives=sorted(backing.get(key,[]))))
                    if key:records[key].setdefault('pool_ids',[]).append(pool['id'])
                vdevs(node['children'],kind)
        vdevs(pool['topology']);pool['members']=members
        pool['roles']=sorted({r for d in pool['datasets'] for r in d['roles']})
        pool['backing_drives']=sorted({disk for m in members if m['role'] in {'data','special','dedup'} for disk in m['backing_drives']})
        for dataset in pool['datasets']:
            dataset['pool_id']=pool['id']
    for node in records.values():
        node['member_of']=list(dict.fromkeys([records[c]['device'] for c in node['child_ids'] if c in logical]+node.get('pool_ids',[])))
    for drive in snapshot['drives']:
        disk=drive['device'];desc=[n for n in records.values() if disk in n['backing_drives']]
        drive['roles']=sorted({r for n in desc for r in n['roles']} | {r for p in pools if disk in p['backing_drives'] for r in p['roles']})
        drive['memberships']=[{'id':n['filesystem_id'],'name':n['device']} for n in desc if n['logical']]+[{'id':p['id'],'name':p['name']} for p in pools if disk in p['backing_drives']]
        # Physical cards show physical partitions only; logical filesystems live in a single shared section.
        drive['display_nodes']=[n for n in desc if not n['logical'] and (n['type']=='part' or n['fstype'] or n['mounts'])]
    snapshot.update(volumes=list(logical.values()),pools=pools,zfs_status=zfs_status)
    snapshot['notices'] = ['ZFS pool details are unavailable; shared ZFS capacity is excluded from totals.'] if zfs_status!='available' and (any(n.get('fstype')=='zfs_member' for n in records.values()) or any(m['fstype']=='zfs' for m in mounts)) else []
    scan_nodes=[n for n in records.values() if n.get('usage') and not n.get('auxiliary')]
    # ZFS datasets get category scans using their actual host mount identity. Pool accounting stays separate.
    for pool in pools:
        for d in pool['datasets']:
            if d['type']=='filesystem' and d['mounts'] and d['filesystem_id']:
                scan_nodes.append(dict(d,usage=True))
    snapshot['scan_targets']=scan_nodes
    total_used=total_available=0
    zfs_ids={d['filesystem_id'] for p in pools for d in p['datasets']}
    for n in records.values():
        if n.get('usage') and not n.get('auxiliary') and n['filesystem_id'] not in zfs_ids and not n.get('pool_ids'):
            total_used+=n['usage']['used_bytes'];total_available+=n['usage']['available_bytes']
    for pool in pools:
        if pool['accounting']:
            total_used+=pool['accounting']['used_bytes'] or 0;total_available+=pool['accounting']['available_bytes'] or 0
    snapshot['totals']={'used_bytes':total_used,'available_bytes':total_available}
    return snapshot
