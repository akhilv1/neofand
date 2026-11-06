import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from portainer_collect import normalize,byte_count,percent,write_snapshot,main

class DockerSnapshotTests(unittest.TestCase):
 def test_metrics_and_allowlist(self):
  ident='a'*64
  result=normalize([dict(id=ident,name='sample',image='portainer/portainer-ce:lts',state='running',status='Up',project='example',Env=['PASSWORD=secret'],Command='secret')],[dict(id=ident,restart_count=2,memory_limit_bytes=0,health='healthy')],[dict(ID=ident,CPUPerc='125.5%',MemUsage='10MiB / 1GiB',NetIO='1kB / 2kB',BlockIO='0B / 2kB')],dict(version='1',cpus=4,memory_bytes=100))
  c=result['containers'][0];self.assertEqual(c['memory_used_bytes'],10485760);self.assertEqual(c['cpu_pct'],125.5);self.assertTrue(result['portainer_detected']);self.assertNotIn('secret',json.dumps(result));self.assertNotIn('Env',c)
 def test_unavailable_is_not_zero_and_stopped_has_no_stats(self):
  self.assertIsNone(percent('NaN%'));self.assertIsNone(percent(None));self.assertEqual(byte_count('2.5GiB'),2684354560);self.assertIsNone(byte_count('unavailable'))
  ident='b'*64;c=normalize([dict(id=ident,state='exited')],[],[dict(ID=ident,CPUPerc='99%')],{})['containers'][0];self.assertIsNone(c['cpu_pct']);self.assertIsNone(c['memory_limit_bytes'])
 def test_symlink_and_writable_parent_refused(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);victim=root/'victim';victim.write_text('untouched');target=root/'portainer.json';target.symlink_to(victim)
   with self.assertRaises(PermissionError):write_snapshot(target,{})
   self.assertEqual(victim.read_text(),'untouched');target.unlink();root.chmod(0o777)
   with self.assertRaises(PermissionError):write_snapshot(target,{})
 def test_failed_scan_retains_timestamp_and_marks_stale(self):
  with tempfile.TemporaryDirectory() as temp:
   path=Path(temp)/'portainer.json';path.write_text(json.dumps(dict(timestamp=123,containers=[dict(name='example')],status='ok')))
   with patch('portainer_collect.collect',side_effect=OSError('private connection error')):main(path)
   d=json.loads(path.read_text());self.assertEqual(d['timestamp'],123);self.assertEqual(d['status'],'unavailable');self.assertNotIn('private connection error',json.dumps(d))
if __name__=='__main__':unittest.main()
