"""Read-only Linux host sampler. No fan control, Docker socket or privileged probes."""
import csv
import json
import math
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import time


def read(path, default=''):
    try:
        return Path(path).read_text().strip()
    except OSError:
        return default


def cpu_times():
    values = [int(x) for x in read('/proc/stat').splitlines()[0].split()[1:9]]
    return sum(values), values[3] + values[4]


def cpu_percent(previous, current):
    total, idle = current[0]-previous[0], current[1]-previous[1]
    return max(0, min(100, 100*(total-idle)/total)) if total > 0 else None


def memory_snapshot():
    mem = {k:int(v.split()[0])*1024 for k,v in (line.split(':',1) for line in read('/proc/meminfo').splitlines())}
    pressure = {}
    for line in read('/proc/pressure/memory').splitlines():
        kind,*fields=line.split()
        pressure[kind]={k:float(v) for k,v in (item.split('=') for item in fields)}
    total=mem['MemTotal']; available=mem['MemAvailable']; used=total-available
    return dict(timestamp=time.time(), total_bytes=total, used_bytes=used, free_bytes=mem['MemFree'], available_bytes=available,
                cache_bytes=max(0,mem.get('Cached',0)+mem.get('SReclaimable',0)-mem.get('Shmem',0)),
                swap_used_bytes=mem.get('SwapTotal',0)-mem.get('SwapFree',0), swap_total_bytes=mem.get('SwapTotal',0),
                used_pct=used/total*100, pressure_some_avg10=pressure.get('some',{}).get('avg10'),
                pressure_full_avg10=pressure.get('full',{}).get('avg10'))


def gpu_rows(output):
    result=[]
    fields=[('temp_c','C',1),('power_w','W',1),('fan_pct','%',1),('util_gpu_pct','%',1),
            ('util_mem_pct','%',1),('mem_used_bytes','B',1048576),('mem_total_bytes','B',1048576)]
    for row in csv.reader(output.splitlines(),skipinitialspace=True):
        if len(row)!=10: continue
        index=int(row[0]); metrics=[]
        for (name,unit,scale),value in zip(fields,row[3:]):
            try: value=float(value)*scale
            except ValueError: continue
            if math.isfinite(value): metrics.append(('gpu',f'gpu{index}_{name}',unit,value))
        result.append((dict(index=index,name=row[1],pci_bus=row[2],physical_slot=None),metrics))
    return result


def atomic_json(path, data):
    tmp=path.with_suffix('.tmp')
    tmp.write_text(json.dumps(data,allow_nan=False))
    tmp.chmod(0o644); os.replace(tmp,path)


def collect(previous):
    now=time.time(); metrics=[]; warnings=[]
    def add(sub,name,unit,value):
        if value is not None and math.isfinite(value):
            metrics.append(dict(subsystem=sub,name=name,unit=unit,ts=now,value=value))
    current=cpu_times(); add('host','cpu_pct','%',cpu_percent(previous,current))
    memory=memory_snapshot(); add('host','mem_used_pct','%',memory['used_pct'])
    for hwmon in sorted(Path('/sys/class/hwmon').glob('hwmon*')):
        chip=read(hwmon/'name',hwmon.name)
        # PCI/device basename is stable across hwmon enumeration changes.
        device=(hwmon/'device').resolve().name
        for path in sorted(hwmon.glob('temp*_input')):
            label=read(path.with_name(path.stem.replace('_input','_label')),path.stem.replace('_input',''))
            name=re.sub(r'[^a-zA-Z0-9_]+','_',f'{chip}_{device}_{label}')
            try: add('hwmon',name,'C',float(path.read_text())/1000)
            except (OSError,ValueError): pass
        for path in sorted(hwmon.glob('fan*_input')):
            name=re.sub(r'[^a-zA-Z0-9_]+','_',f'{chip}_{device}_{path.stem}')
            try: add('fan',name,'RPM',float(path.read_text()))
            except (OSError,ValueError): pass
    gpus=[]
    try:
        r=subprocess.run(['nvidia-smi','--query-gpu=index,name,pci.bus_id,temperature.gpu,power.draw,fan.speed,utilization.gpu,utilization.memory,memory.used,memory.total','--format=csv,noheader,nounits'],capture_output=True,text=True,check=True,timeout=5)
        for gpu,rows in gpu_rows(r.stdout):
            gpus.append(gpu)
            for row in rows: add(*row)
    except (OSError,ValueError,subprocess.SubprocessError): warnings.append('NVIDIA telemetry unavailable for this sample.')
    dmi='/sys/class/dmi/id/'
    family=read(dmi+'product_family'); version=read(dmi+'product_version')
    display=next((v for v in (family,version,read(dmi+'product_name')) if v and v.lower() not in ('default string','to be filled by o.e.m.','system product name','system version')),'Linux host')
    platform='lenovo-p620' if 'ThinkStation P620' in (family,version) else 'linux'
    return current,dict(timestamp=now,latest=metrics,memory=memory,gpus=gpus,warnings=warnings,
                        host=dict(hostname=read('/proc/sys/kernel/hostname'),platform=platform,display_name=display,product_family=family,
                                  product_version=version,product_name=read(dmi+'product_name'),vendor=read(dmi+'sys_vendor'),
                                  cpu=next((x.split(':',1)[1].strip() for x in read('/proc/cpuinfo').splitlines() if x.startswith('model name')),'Unknown'),
                                  boot_id=read('/proc/sys/kernel/random/boot_id'),uptime_seconds=float(read('/proc/uptime','0').split()[0])))


def main(directory):
    folder=Path(directory);folder.mkdir(parents=True,exist_ok=True)
    db=sqlite3.connect(folder/'linux.sqlite')
    db.execute('PRAGMA journal_mode=WAL')
    db.execute('CREATE TABLE IF NOT EXISTS samples (ts INTEGER, subsystem TEXT, name TEXT, unit TEXT, value REAL)')
    db.execute('CREATE INDEX IF NOT EXISTS metric_time ON samples(subsystem,name,ts)')
    db.execute('CREATE INDEX IF NOT EXISTS sample_time ON samples(ts)')
    db.commit()
    previous=cpu_times(); last_store=0;last_prune=0
    while True:
        started=time.monotonic()
        try:
            previous,data=collect(previous)
            now=int(data['timestamp'])
            if now-last_store>=30:
                db.executemany('INSERT INTO samples VALUES (?,?,?,?,?)',[(now,m['subsystem'],m['name'],m['unit'],m['value']) for m in data['latest']])
                db.commit();last_store=now
            if now-last_prune>=3600:
                db.execute('DELETE FROM samples WHERE ts < ?',(now-30*86400,));db.commit();last_prune=now
            data['history_since']=db.execute('SELECT min(ts) FROM samples').fetchone()[0]
            atomic_json(folder/'linux.json',data)
        except Exception as error:
            # Keep the previous snapshot timestamp on failure so clients detect staleness.
            print('Sampler failed: '+type(error).__name__,flush=True)
        time.sleep(max(.1,2-(time.monotonic()-started)))

if __name__=='__main__': main(sys.argv[1])
