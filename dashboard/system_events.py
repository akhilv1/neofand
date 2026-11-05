"""Bounded, read-only Ubuntu journal snapshot. Never treats a gap as proven power loss."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

WINDOW = 30 * 86400
LIMIT = 2000
FIELDS = 'MESSAGE,PRIORITY,SYSLOG_IDENTIFIER,_SYSTEMD_UNIT,_BOOT_ID,__REALTIME_TIMESTAMP,__CURSOR'
POWER_UNITS = ('nut-monitor.service', 'nut-server.service', 'apcupsd.service')

def text(value):
    if isinstance(value, list):
        return ' '.join(map(str, value))[:4096]
    return str(value or '')[:4096]

def scrub(value):
    value = text(value)
    value = re.sub(r'(?i)\b(password|passwd|token|secret|api[_-]?key|authorization)\s*[:=]\s*\S+', r'\1=[redacted]', value)
    value = re.sub(r'(?i)\bBearer\s+\S+', 'Bearer [redacted]', value)
    value = re.sub(r'(https?://)[^/\s@]+@', r'\1[redacted]@', value)
    value = re.sub(r'(https?://[^\s?]+)\?\S+', r'\1?[redacted]', value)
    return ''.join(c for c in value if c in '\n\t' or ord(c) >= 32)[:1000]

def journal(args):
    result = subprocess.run(['journalctl','--no-pager',*args],capture_output=True,text=True,timeout=15)
    if result.returncode or 'not seeing messages' in result.stderr.lower():
        raise RuntimeError('System journal unavailable or not fully readable')
    return result.stdout

def records(args):
    return [json.loads(line) for line in journal(['-o','json','--output-fields='+FIELDS,*args]).splitlines() if line.strip()]

def record_event(row, power=False):
    try:
        ts=int(row.get('__REALTIME_TIMESTAMP',0))/1_000_000
        priority=int(row.get('PRIORITY',6))
    except (TypeError,ValueError):
        return None
    if not ts:return None
    unit=text(row.get('_SYSTEMD_UNIT') or row.get('SYSLOG_IDENTIFIER') or 'journal')
    message=scrub(row.get('MESSAGE'))
    if not message:return None
    kind='ubuntu';title='Ubuntu journal';severity='error' if priority<=3 else 'warning' if priority==4 else 'info';confidence='recorded'
    if power:
        low=message.lower()
        if re.search(r'on battery|power failure|mains.*(lost|fail)|utility.*fail',low):
            kind='power';title='UPS reports utility power loss';severity='warning'
        elif re.search(r'on line|on-line|power.*restor|mains.*restor|utility.*restor',low):
            kind='power';title='UPS reports utility power restored';severity='info'
        else:
            kind='power';title='UPS event'
    ident=text(row.get('__CURSOR')) or f'{ts}:{unit}:{message}'
    return {'id':hashlib.sha256(ident.encode()).hexdigest()[:24],'ts':ts,'source':'Ubuntu','kind':kind,'title':title,'unit':unit,'message':message,'severity':severity,'confidence':confidence}

def shutdown_evidence(rows):
    for row in rows: # newest first
        source=text(row.get('SYSLOG_IDENTIFIER'))
        if source not in ('systemd','systemd-shutdown'):continue
        msg=text(row.get('MESSAGE')).lower()
        if re.search(r'rebooting|reached target.*(system reboot|reboot\.target)',msg):return 'reboot'
        if re.search(r'powering off|reached target.*(system power off|poweroff\.target)',msg):return 'poweroff'
        if re.search(r'shutting down|reached target.*(system shutdown|shutdown\.target)',msg):return 'shutdown'
    return None

def boot_events(boots,evidence,current_boot,since):
    out=[]
    ordered=sorted(boots,key=lambda b:int(b['first_entry']))
    for i,b in enumerate(ordered):
        boot=b['boot_id'];start=int(b['first_entry'])/1e6;end=int(b['last_entry'])/1e6
        def add(suffix,ts,title,message,severity='info',confidence='recorded'):
            if ts>=since:out.append({'id':boot+suffix,'ts':ts,'source':'Ubuntu','kind':'boot','title':title,'unit':'system','message':message,'severity':severity,'confidence':confidence})
        add('-boot',start,'Boot observed','First retained journal record for boot '+boot[:8]+'. Journal retention may start after the actual boot.')
        if boot!=current_boot:
            kind=evidence.get(boot)
            if kind:
                add('-shutdown',end,{'reboot':'Reboot sequence recorded','poweroff':'Power-off sequence recorded','shutdown':'Shutdown sequence recorded'}[kind], 'The previous boot journal contains a systemd shutdown marker; time shown is its last retained record.')
            elif i+1<len(ordered):
                next_start=int(ordered[i+1]['first_entry'])/1e6
                add('-unexpected',next_start,'Restart without a recorded clean shutdown','No shutdown marker was found in the retained tail of the previous boot. Possible causes include power loss, crash, forced reset or missing journal history; this is not a confirmed power outage.','warning','inferred')
    return out

def collect(previous=None,now=None):
    now=time.time() if now is None else now;since=now-WINDOW
    previous=previous or {};warnings=[];events=[]
    rows=records(['--since','@'+str(int(since)),'-p','0..4','-n',str(LIMIT),'--reverse'])
    if len(rows)>=LIMIT:warnings.append(f'Journal results limited to the newest {LIMIT} warnings/errors. Narrow the window with journalctl for complete history.')
    for row in rows:
        event=record_event(row)
        if event:events.append(event)
    ups_args=[]
    for unit in POWER_UNITS:ups_args+=['-u',unit]
    ups=records(['--since','@'+str(int(since)),'-n','300','--reverse',*ups_args])
    for row in ups:
        event=record_event(row,True)
        if event:events.append(event)
    if len(ups)>=300:warnings.append('UPS history capped at the newest 300 records.')
    boots=json.loads(journal(['--list-boots','-o','json']))
    boots=[b for b in boots if int(b['last_entry'])/1e6>=since][-32:]
    current=Path('/proc/sys/kernel/random/boot_id').read_text().strip().replace('-','')
    cache=previous.get('boot_evidence',{})
    evidence={}
    for b in boots:
        bid=b['boot_id']
        if not re.fullmatch(r'[0-9a-f]{32}',bid):continue
        if bid==current:continue
        if bid in cache:evidence[bid]=cache[bid]
        else:evidence[bid]=shutdown_evidence(records(['-b',bid,'-n','300','--reverse']))
    events+=boot_events(boots,evidence,current,since)
    unique={e['id']:e for e in events}
    return {'timestamp':now,'window_start':since,'status':'ok','events':sorted(unique.values(),key=lambda e:e['ts'],reverse=True),'warnings':warnings,'boot_evidence':evidence,'ups_records':len(ups),'power_detection':'UPS events are reported when available. Unexpected restarts are inferred, never proof of a power outage.'}

def main(path):
    target=Path(path)
    try:previous=json.loads(target.read_text())
    except (OSError,ValueError):previous={}
    try:data=collect(previous)
    except (OSError,ValueError,RuntimeError,subprocess.TimeoutExpired):
        data={**previous,'status':'unavailable','error':'Ubuntu journal collection failed; retained events may be stale.','attempted_at':time.time()}
        data.setdefault('timestamp',0);data.setdefault('events',[])
    target.parent.mkdir(parents=True,exist_ok=True)
    tmp=target.with_suffix('.tmp');tmp.write_text(json.dumps(data,ensure_ascii=False)+'\n');tmp.chmod(0o644);os.replace(tmp,target)

if __name__=='__main__':main(sys.argv[1])
