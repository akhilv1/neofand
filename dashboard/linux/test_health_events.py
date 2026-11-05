import json,tempfile,unittest
from pathlib import Path
from linux_api import host_events
class HealthEventsTests(unittest.TestCase):
 def test_alerts_survive_missing_journal_and_baseline_is_private(self):
  with tempfile.TemporaryDirectory() as t:
   root=Path(t);(root/'drive-health-history.json').write_text(json.dumps({'timestamp':1,'states':{'private':'baseline'},'events':[{'ts':1,'kind':'storage','title':'Drive warning'}]}))
   data=host_events(root)
   self.assertEqual(data['events'][0]['kind'],'storage')
   self.assertNotIn('states',data)
