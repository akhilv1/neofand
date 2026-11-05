"""Container HTTP reader for host-written telemetry; no control endpoints."""
from contextlib import closing
import threading
import json
import math
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sqlite3
import time
from urllib.parse import urlparse, parse_qs

DATA=Path(os.environ.get('NEOFAND_DATA_DIR','/host/telemetry'))

def telemetry_file(name, directory=None):
    # Proxmox separates unprivileged telemetry from root-owned SMART state.
    directory=DATA if directory is None else Path(directory)
    nested=directory/'telemetry'/name
    return nested if nested.exists() else directory/name


def snapshot():
    return json.loads(telemetry_file('linux.json').read_text())


def host_events(directory=DATA):
    try:data=json.loads(telemetry_file('system-events.json',directory).read_text())
    except (OSError,ValueError):data={'status':'unavailable','timestamp':0,'window_start':0,'events':[],'warnings':['Ubuntu journal readings unavailable.']}
    data.pop('boot_evidence',None)
    try:
        health=json.loads((directory/'drive-health-history.json').read_text())
        data.setdefault('events',[]).extend(health.get('events',[]))
        data.setdefault('warnings',[]).append(f"Drive health alerts retained up to {health.get('retention_days',365)} days; maximum 10,000 records.")
        if time.time()-health.get('timestamp',0)>180:data['warnings'].append('Drive health monitoring is stale; retained alerts are shown.')
    except FileNotFoundError:pass
    except (OSError,ValueError):data.setdefault('warnings',[]).append('Drive health alert history unavailable.')
    data['events']=sorted(data.get('events',[]),key=lambda e:e.get('ts',0),reverse=True)
    return data


def series(query, database):
    now=int(time.time())
    start=max(now-30*86400,int(query.get('from',[now-21600])[0]))
    end=min(now,int(query.get('to',[now])[0]))
    if end<start: raise ValueError('Invalid time range')
    step=max(30,int(query.get('step',[30])[0]),math.ceil((end-start)/1000))
    raw=query.get('name',[''])[0];sub=query.get('subsystem',[''])[0]
    if len(raw)>4096 or len(sub)>128:raise ValueError('Query too large')
    names=list(dict.fromkeys(raw.split(',')))
    if len(names)>64 or any(len(n)>128 for n in names):raise ValueError('Too many metrics')
    deadline=time.monotonic()+2.0
    with closing(sqlite3.connect(Path(database).resolve().as_uri()+'?mode=ro',uri=True,timeout=1)) as db:
        db.set_progress_handler(lambda: int(time.monotonic()>deadline),1000)
        result=[]
        for name in names:
            rows=db.execute('SELECT (ts / ?) * ?, avg(value), min(value), max(value) FROM samples WHERE subsystem=? AND name=? AND ts BETWEEN ? AND ? GROUP BY ts / ? ORDER BY ts / ?', (step,step,sub,name,start,end,step,step)).fetchall()
            result.append(dict(name=name,points=rows))
    return dict(series=result)


class BoundedHTTPServer(ThreadingHTTPServer):
    daemon_threads=True
    request_queue_size=16
    def __init__(self,*args,**kwargs):
        self.slots=threading.BoundedSemaphore(8)
        super().__init__(*args,**kwargs)
    def process_request(self,request,address):
        if not self.slots.acquire(blocking=False):
            self.shutdown_request(request);return
        try:super().process_request(request,address)
        except BaseException:
            self.slots.release();raise
    def process_request_thread(self,request,address):
        try:super().process_request_thread(request,address)
        finally:self.slots.release()
    def get_request(self):
        request,address=super().get_request();request.settimeout(5)
        return request,address


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if len(self.path)>8192:self.send_error(414);return
        url=urlparse(self.path)
        try:
            path=url.path
            if path=='/api/series': data=series(parse_qs(url.query,max_num_fields=16),telemetry_file('linux.sqlite'))
            elif path=='/api/host-events':
                data=host_events()
            elif path=='/api/proxmox': data=json.loads((DATA/'proxmox.json').read_text())
            elif path=='/api/host-storage': data=json.loads(telemetry_file('storage.json').read_text())
            elif path=='/api/events': data={'events':[]}
            elif path in ('/api/latest','/api/metrics','/api/host-memory','/api/host-info','/health'):
                snap=snapshot()
                if path=='/api/latest': data=snap['latest']
                elif path=='/api/metrics': data=[{k:m[k] for k in ('subsystem','name','unit')} for m in snap['latest']]
                elif path=='/api/host-memory': data=snap['memory']
                elif path=='/health':
                    if time.time()-snap['timestamp']>30: self.send_error(503,'Host sampler stale');return
                    data={'status':'ok'}
                else: data={**{k:v for k,v in snap.items() if k not in ('latest','memory')},'proxmox_available':(DATA/'proxmox.json').exists()}
            else: self.send_error(404);return
            body=json.dumps(data,allow_nan=False).encode()
        except (OSError,sqlite3.Error): self.send_error(503,'Host telemetry unavailable');return
        except (ValueError,KeyError,OverflowError): self.send_error(400,'Invalid request or telemetry');return
        self.send_response(200)
        self.send_header('Content-Type','application/json');self.send_header('Cache-Control','no-store')
        self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def log_message(self,*args): pass

if __name__=='__main__': BoundedHTTPServer((os.environ.get('NEOFAND_BIND','127.0.0.1'),int(os.environ.get('NEOFAND_PORT','8080'))),Handler).serve_forever()
