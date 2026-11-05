"""Container HTTP reader for host-written telemetry; no control endpoints."""
import json
import math
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sqlite3
import time
from urllib.parse import urlparse, parse_qs

DATA=Path(os.environ.get('NEOFAND_DATA_DIR','/host/telemetry'))

def snapshot():
    return json.loads((DATA/'linux.json').read_text())


def series(query, database):
    now=int(time.time())
    start=max(now-30*86400,int(query.get('from',[now-21600])[0]))
    end=min(now,int(query.get('to',[now])[0]))
    if end<start: raise ValueError('Invalid time range')
    step=max(30,int(query.get('step',[30])[0]),math.ceil((end-start)/1000))
    names=query.get('name',[''])[0].split(',')[:64];sub=query.get('subsystem',[''])[0]
    with sqlite3.connect(f'file:{database}?mode=ro',uri=True,timeout=5) as db:
        result=[]
        for name in names:
            rows=db.execute('SELECT (ts / ?) * ?, avg(value), min(value), max(value) FROM samples WHERE subsystem=? AND name=? AND ts BETWEEN ? AND ? GROUP BY ts / ? ORDER BY ts / ?', (step,step,sub,name,start,end,step,step)).fetchall()
            result.append(dict(name=name,points=rows))
    return dict(series=result)


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url=urlparse(self.path)
        try:
            path=url.path
            if path=='/api/series': data=series(parse_qs(url.query),DATA/'linux.sqlite')
            elif path=='/api/host-events':
                data=json.loads((DATA/'system-events.json').read_text());data.pop('boot_evidence',None)
            elif path=='/api/proxmox': data=json.loads((DATA/'proxmox.json').read_text())
            elif path=='/api/host-storage': data=json.loads((DATA/'storage.json').read_text())
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

if __name__=='__main__': ThreadingHTTPServer((os.environ.get('NEOFAND_BIND','0.0.0.0'),int(os.environ.get('NEOFAND_PORT','8080'))),Handler).serve_forever()
