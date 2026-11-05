"""Read-only host RAM and PSI snapshot for the hardware dashboard."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from time import time


def snapshot():
    mem = {}
    for line in Path('/host/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        mem[key] = int(value.split()[0]) * 1024
    pressure = {}
    for line in Path('/host/memory-pressure').read_text().splitlines():
        kind, *fields = line.split()
        pressure[kind] = {key: float(value) for key, value in (field.split('=') for field in fields)}
    total = mem['MemTotal']
    available = mem['MemAvailable']
    used = total - available
    return {
        'timestamp': int(time()),
        'total_bytes': total,
        'used_bytes': used,
        'free_bytes': mem['MemFree'],
        'available_bytes': available,
        'cache_bytes': max(0, mem.get('Cached', 0) + mem.get('SReclaimable', 0) - mem.get('Shmem', 0)),
        'swap_used_bytes': mem.get('SwapTotal', 0) - mem.get('SwapFree', 0),
        'swap_total_bytes': mem.get('SwapTotal', 0),
        'used_pct': used / total * 100 if total else 0,
        'pressure_some_avg10': pressure.get('some', {}).get('avg10'),
        'pressure_full_avg10': pressure.get('full', {}).get('avg10'),
    }


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path not in ('/memory', '/storage'):
            self.send_error(404)
            return
        try:
            data = snapshot() if self.path == '/memory' else json.loads(Path('/host/telemetry/storage.json').read_text())
            body = json.dumps(data, allow_nan=False).encode()
        except (OSError, ValueError, KeyError):
            self.send_error(503, 'Host memory readings unavailable')
            return
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


if __name__ == '__main__':
    ThreadingHTTPServer(('0.0.0.0', 8080), Handler).serve_forever()
