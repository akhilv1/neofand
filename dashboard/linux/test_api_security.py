from pathlib import Path
import sqlite3
import tempfile
import threading
import unittest
from unittest.mock import patch
from linux_api import series, telemetry_file, BoundedHTTPServer, Handler

class APISecurityTests(unittest.TestCase):
    def test_nested_layout_and_flat_compatibility(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'linux.json').write_text('{}')
            self.assertEqual(telemetry_file('linux.json',root),root/'linux.json')
            (root/'telemetry').mkdir();(root/'telemetry/linux.json').write_text('{}')
            self.assertEqual(telemetry_file('linux.json',root),root/'telemetry/linux.json')
    def test_query_dedup_and_time_budget(self):
        with tempfile.TemporaryDirectory() as temp:
            db=Path(temp)/'sample.db'
            with sqlite3.connect(db) as c:
                c.execute('CREATE TABLE samples(ts INTEGER,subsystem TEXT,name TEXT,value REAL)')
                c.executemany('INSERT INTO samples VALUES(?,?,?,?)',[(1,'cpu','load',1)]*3000)
            self.assertEqual(len(series({'name':['load,load']},db)['series']),1)
            with patch('linux_api.time.monotonic',side_effect=[0,10,10,10]):
                with self.assertRaises(sqlite3.OperationalError):series({'name':['load']},db)
    def test_query_bounds(self):
        with self.assertRaises(ValueError):series({'name':[','.join(str(i) for i in range(65))]},'/unused')
        with self.assertRaises(ValueError):series({'subsystem':['x'*129]},'/unused')
    def test_connection_cap_closes_excess(self):
        with BoundedHTTPServer(('127.0.0.1',0),Handler,bind_and_activate=False) as server:
            for _ in range(8):self.assertTrue(server.slots.acquire(False))
            with patch.object(server,'shutdown_request') as close:
                server.process_request(object(),('127.0.0.1',1234));close.assert_called_once()

if __name__=='__main__':unittest.main()
