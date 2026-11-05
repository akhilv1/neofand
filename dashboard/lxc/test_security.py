import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from proxmox_collect import atomic_json, read_storage, save_health_history

class BoundaryTests(unittest.TestCase):
    def test_old_predictable_temp_symlink_does_not_clobber(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);victim=root/'victim';victim.write_text('unchanged')
            (root/'history.tmp').symlink_to(victim)
            save_health_history(root/'history.json',[])
            self.assertEqual(victim.read_text(),'unchanged')
            self.assertFalse((root/'history.json').is_symlink())
            self.assertEqual(json.loads((root/'history.json').read_text())['events'],[])
    def test_untrusted_parent_and_destination_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);root.chmod(0o777)
            with self.assertRaises(PermissionError):atomic_json(root/'snapshot.json',{})
            root.chmod(0o700);(root/'snapshot.json').symlink_to(root/'victim')
            with self.assertRaises(PermissionError):atomic_json(root/'snapshot.json',{})
    def test_untrusted_storage_symlink_and_oversize_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);target=root/'data';target.write_text('{}')
            (root/'link').symlink_to(target)
            with self.assertRaises(OSError):read_storage(root/'link')
            with target.open('wb') as f:f.truncate(4*1024*1024+1)
            with self.assertRaises(ValueError):read_storage(target)

if __name__=='__main__':unittest.main()
