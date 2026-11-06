"""Read-only local Docker snapshot for the Portainer dashboard; no API credentials."""
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

MAX_CONTAINERS=128
PS_FORMAT='{"id":{{json .ID}},"name":{{json .Names}},"image":{{json .Image}},"state":{{json .State}},"status":{{json .Status}},"project":{{json (.Label "com.docker.compose.project")}}}'
INSPECT_FORMAT='{"id":{{json .Id}},"restart_count":{{.RestartCount}},"memory_limit_bytes":{{.HostConfig.Memory}},"health":{{with index .State "Health"}}{{json .Status}}{{else}}null{{end}}}'

def docker(*args):
    # Explicit local socket: never follow DOCKER_HOST/context to another machine.
    with tempfile.TemporaryFile() as output:
        result=subprocess.run(['docker','--host','unix:///var/run/docker.sock',*args],stdout=output,stderr=subprocess.DEVNULL,timeout=15,check=True,env={**os.environ,'DOCKER_CONFIG':'/nonexistent'})
        if output.tell()>4*1024*1024:raise ValueError('Inventory exceeds snapshot size limit')
        output.seek(0);return output.read().decode()

def percent(value):
    try:result=float(str(value).rstrip('%'))
    except (TypeError,ValueError):return None
    return result if math.isfinite(result) and result>=0 else None

def byte_count(value):
    match=re.fullmatch(r'\s*([0-9]+(?:\.[0-9]+)?)\s*([KMGT]?i?B)\s*',str(value),re.I)
    if not match:return None
    number,unit=match.groups();unit=unit.lower();power={'b':0,'kb':1,'kib':1,'mb':2,'mib':2,'gb':3,'gib':3,'tb':4,'tib':4}[unit]
    return round(float(number)*(1024 if 'i' in unit else 1000)**power)

def normalize(containers,details,stats,info):
    details={row.get('id'):row for row in details};stats={row.get('ID'):row for row in stats}
    rows=[]
    for row in containers[:MAX_CONTAINERS]:
        ident=row.get('id','')
        if not re.fullmatch('[a-f0-9]{64}',ident):continue
        state=row.get('state','unknown');stat=stats.get(ident,{}) if state=='running' else {}
        detail=details.get(ident,{})
        memory=str(stat.get('MemUsage','')).partition('/')[0]
        rows.append(dict(id=ident,name=str(row.get('name',''))[:160],image=str(row.get('image',''))[:256],
            project=str(row.get('project',''))[:160] or None,state=state if state in ('running','exited','created','paused','restarting','removing','dead') else 'unknown',
            status=str(row.get('status',''))[:160],health=detail.get('health') if detail.get('health') in ('healthy','unhealthy','starting') else None,
            restart_count=detail.get('restart_count'),memory_limit_bytes=detail.get('memory_limit_bytes'),
            cpu_pct=percent(stat.get('CPUPerc')),memory_used_bytes=byte_count(memory),
            network_io=str(stat.get('NetIO',''))[:80] or None,block_io=str(stat.get('BlockIO',''))[:80] or None))
    portainer=any(re.search(r'(?:^|/)portainer/portainer-(?:ce|ee)(?::|@|$)',c['image']) for c in rows)
    return dict(timestamp=time.time(),status='ok',source='local-docker',portainer_detected=portainer,
        engine_version=str(info.get('version',''))[:64],host_cpus=info.get('cpus'),host_memory_bytes=info.get('memory_bytes'),
        containers=rows,truncated=len(containers)>MAX_CONTAINERS,warnings=[])

def collect():
    inventory=[json.loads(line) for line in docker('ps','--all','--no-trunc','--format',PS_FORMAT).splitlines() if line]
    ids=[row['id'] for row in inventory[:MAX_CONTAINERS] if re.fullmatch('[a-f0-9]{64}',row.get('id',''))]
    info=json.loads(docker('info','--format','{"version":{{json .ServerVersion}},"cpus":{{.NCPU}},"memory_bytes":{{.MemTotal}}}'))
    warnings=[]
    try:details=[json.loads(line) for line in docker('inspect','--format',INSPECT_FORMAT,*ids).splitlines()] if ids else []
    except (OSError,ValueError,subprocess.SubprocessError):details=[];warnings.append('Health and restart details unavailable for this sample.')
    running=[r['id'] for r in inventory[:MAX_CONTAINERS] if r.get('state')=='running' and r.get('id') in ids]
    try:stats=[json.loads(line) for line in docker('stats','--no-stream','--no-trunc','--format','{{json .}}',*running).splitlines()] if running else []
    except (OSError,ValueError,subprocess.SubprocessError):stats=[];warnings.append('Container resource readings unavailable for this sample.')
    result=normalize(inventory,details,stats,info);result['warnings']=warnings;return result

def write_snapshot(path,data):
    path=Path(path);parent=path.parent
    # This service and its output directory must have the same trusted owner.
    st=parent.stat()
    if parent.is_symlink() or st.st_uid!=os.geteuid() or st.st_mode&0o022 or path.is_symlink():raise PermissionError('Untrusted output path')
    fd,temp=tempfile.mkstemp(prefix='.portainer-',dir=parent)
    try:
        with os.fdopen(fd,'w') as out:
            os.fchmod(out.fileno(),0o644);json.dump(data,out,allow_nan=False);out.flush();os.fsync(out.fileno())
        os.replace(temp,path)
    finally:
        if os.path.exists(temp):os.unlink(temp)

def main(path):
    try:data=collect()
    except (OSError,ValueError,KeyError,subprocess.SubprocessError):
        # Preserve useful last readings, without exposing raw command errors.
        try:
            if Path(path).is_symlink():raise ValueError('Symlink snapshot')
            data=json.loads(Path(path).read_text())
        except (OSError,ValueError):data={'timestamp':0,'containers':[]}
        data.update(status='unavailable',attempted_at=time.time(),warnings=['Local Docker inventory unavailable; any retained readings may be stale.'])
    write_snapshot(path,data)

if __name__=='__main__':main(sys.argv[1])
