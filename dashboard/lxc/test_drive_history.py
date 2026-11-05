import json
import tempfile
import unittest
from pathlib import Path
from proxmox_collect import normalize_smart,health_history,save_health_history

def drive(state='passed',errors=0):
 return {'device':'/dev/sda','smart':{'device_id':'stable-a','state':state,'model':'Test disk','media_errors':errors,'history_warnings':[]}}

class DriveHistoryTests(unittest.TestCase):
 def test_baseline_and_restart_do_not_duplicate_alerts(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/'history.json'
   save_health_history(path,[drive('warning',1)])
   first=json.loads(path.read_text())
   self.assertEqual(len(first['events']),1)
   save_health_history(path,[drive('warning',1)])
   self.assertEqual(len(json.loads(path.read_text())['events']),1)
 def test_error_increase_and_recovery(self):
  d=health_history({},[drive()],1)
  d=health_history(d,[drive('warning',1)],2)
  self.assertEqual(d['events'][0]['severity'],'warning')
  d=health_history(d,[drive()],3)
  self.assertEqual(d['events'][0]['severity'],'info')
 def test_sleep_is_neutral_and_unavailable_is_debounced(self):
  d=health_history({},[drive()],1)
  d=health_history(d,[drive('sleeping')],2)
  self.assertEqual(d['events'],[])
  for ts in (3,4):d=health_history(d,[drive('unavailable')],ts)
  self.assertEqual(d['events'],[])
  d=health_history(d,[drive('unavailable')],5)
  self.assertEqual(len(d['events']),1)
  d=health_history(d,[drive()],6)
  self.assertEqual(d['events'][0]['title'],'Drive health readings restored')
 def test_missing_drive_and_retention(self):
  d=health_history({},[drive()],1)
  for ts in (2,3,4):d=health_history(d,[],ts)
  self.assertEqual(d['events'][0]['title'],'Drive no longer detected')
  self.assertEqual(health_history(d,[],100000,1)['events'],[])
 def test_first_wake_up_does_not_report_a_missing_disk(self):
  asleep=drive('sleeping');asleep['smart']['device_id']=None
  d=health_history({},[asleep],1)
  for ts in (2,3,4,5):d=health_history(d,[drive()],ts)
  self.assertEqual(d['events'],[])
  self.assertEqual(len(d['states']),1)
 def test_ata_history_and_sleep(self):
  self.assertEqual(normalize_smart({'smart_status':{'passed':True}},32)['state'],'warning')
  self.assertEqual(normalize_smart({'smartctl':{'messages':[{'string':'Device is in SLEEP mode, exit(3)'}]}},3)['state'],'sleeping')
