import sqlite3
import tempfile
import time
import unittest
from unittest.mock import patch,MagicMock
from pathlib import Path
from linux_telemetry import cpu_percent,gpu_rows
from linux_api import series,Handler

class TelemetryTests(unittest.TestCase):
 def test_cpu_delta_excludes_idle(self):
  self.assertEqual(cpu_percent((1000,600),(1200,650)),75)
  self.assertIsNone(cpu_percent((1000,600),(1000,600)))
 def test_gpu_units_and_missing_values(self):
  gpu,metrics=gpu_rows('0, NVIDIA GeForce RTX 3090, 00000000:61:00.0, 27, 13.07, 0, 0, [N/A], 9, 24576')[0]
  values={r[1]:r[3] for r in metrics}
  self.assertEqual(values['gpu0_mem_total_bytes'],24576*1048576)
  self.assertEqual(values['gpu0_fan_pct'],0)
  self.assertNotIn('gpu0_util_mem_pct',values)
  self.assertIsNone(gpu['physical_slot'])
 def test_no_nan_metric(self):
  _,metrics=gpu_rows('0, NVIDIA, 00, nan, 10, 0, 0, 0, 0, 1')[0]
  self.assertNotIn('gpu0_temp_c',[m[1] for m in metrics])
 def test_history_aggregate_and_bound(self):
  with tempfile.TemporaryDirectory() as tmp:
   path=Path(tmp)/'s.db';db=sqlite3.connect(path);db.execute('CREATE TABLE samples(ts INTEGER,subsystem TEXT,name TEXT,unit TEXT,value REAL)')
   now=int(time.time());bucket=(now//30-2)*30
   db.executemany('INSERT INTO samples VALUES (?,?,?,?,?)',[(bucket+1,'gpu','gpu0_power_w','W',10),(bucket+20,'gpu','gpu0_power_w','W',30),(now-40*86400,'gpu','gpu0_power_w','W',900)])
   db.commit();db.close()
   rows=series({'subsystem':['gpu'],'name':['gpu0_power_w'],'from':[str(bucket)],'to':[str(now)],'step':['30']},path)['series'][0]['points']
   self.assertEqual(rows,[(bucket,20,10,30)])
   self.assertEqual(series({'subsystem':['gpu'],'name':["' OR 1=1 --"]},path)['series'][0]['points'],[])
 def test_stale_snapshot_fails_health(self):
  handler=object.__new__(Handler);handler.path='/health';handler.send_error=MagicMock()
  with patch('linux_api.snapshot',return_value={'timestamp':time.time()-60}):handler.do_GET()
  handler.send_error.assert_called_once_with(503,'Host sampler stale')
 def test_invalid_time_range(self):
  with self.assertRaises(ValueError):series({'from':['99999999999'],'to':['0']},'/unused')

if __name__=='__main__':unittest.main()
