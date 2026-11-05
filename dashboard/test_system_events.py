import unittest
from unittest.mock import patch
import system_events as s

class EventsTests(unittest.TestCase):
 def test_shutdown_requires_system_source(self):
  self.assertIsNone(s.shutdown_evidence([{'SYSLOG_IDENTIFIER':'random-app','MESSAGE':'Rebooting'}]))
  self.assertEqual(s.shutdown_evidence([{'SYSLOG_IDENTIFIER':'systemd','MESSAGE':'Reached target System Reboot.'}]),'reboot')
 def test_unclean_is_inferred_not_outage(self):
  boots=[{'boot_id':'a','first_entry':1e6,'last_entry':2e6},{'boot_id':'b','first_entry':3e6,'last_entry':4e6}]
  events=s.boot_events(boots,{},'b',0)
  inferred=[e for e in events if e['confidence']=='inferred']
  self.assertEqual(len(inferred),1)
  self.assertIn('not a confirmed power outage',inferred[0]['message'])
  self.assertFalse(any(e['id']=='b-unexpected' for e in events))
 def test_clean_marker_removes_inference(self):
  boots=[{'boot_id':'a','first_entry':1e6,'last_entry':2e6},{'boot_id':'b','first_entry':3e6,'last_entry':4e6}]
  events=s.boot_events(boots,{'a':'reboot'},'b',0)
  self.assertTrue(any(e['title']=='Reboot sequence recorded' for e in events))
  self.assertFalse(any(e['confidence']=='inferred' for e in events))
 def test_power_classification_requires_ups_query(self):
  row={'__REALTIME_TIMESTAMP':'1000000','MESSAGE':'UPS on battery','PRIORITY':'6'}
  self.assertEqual(s.record_event(row)['kind'],'ubuntu')
  self.assertEqual(s.record_event(row,True)['title'],'UPS reports utility power loss')
 def test_redaction_and_invalid_values(self):
  self.assertNotIn('abcd',s.scrub('token=abcd https://u:abcd@site/path?secret=abcd'))
  self.assertIsNone(s.record_event({'__REALTIME_TIMESTAMP':'bad','MESSAGE':'oops'}))
 def test_window_bounds(self):
  events=s.boot_events([{'boot_id':'a','first_entry':1e6,'last_entry':2e6}],{},'a',3)
  self.assertEqual(events,[])
 def test_failure_keeps_stale_snapshot(self):
  import tempfile,json
  from pathlib import Path
  with tempfile.TemporaryDirectory() as d:
   p=Path(d)/'events.json';p.write_text(json.dumps({'timestamp':1,'events':[{'id':'kept'}]}))
   with patch.object(s,'collect',side_effect=RuntimeError('failed')):s.main(p)
   data=json.loads(p.read_text());self.assertEqual(data['status'],'unavailable');self.assertEqual(data['timestamp'],1);self.assertEqual(data['events'][0]['id'],'kept')
if __name__=='__main__':unittest.main()
