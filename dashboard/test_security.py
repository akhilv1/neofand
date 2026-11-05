from pathlib import Path
import os
import tempfile
import unittest
from unittest.mock import patch
from export_history import export
from system_events import record_event, scrub

class DataSecurityTests(unittest.TestCase):
    def test_archive_rejects_host_directory_symlink(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'source';source.mkdir();(source/'linux.json').write_text('{}')
            destination=root/'out';destination.mkdir();escape=root/'escape';escape.mkdir()
            (destination/'test-host').symlink_to(escape,target_is_directory=True)
            with self.assertRaises(ValueError):export(source,destination,[],'test-host')
            self.assertEqual(list(escape.iterdir()),[])
    def test_archive_private_permissions(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);source=root/'source';source.mkdir();(source/'linux.json').write_text('{}')
            output=export(source,root/'out',[],'test-host')
            self.assertEqual(output.stat().st_mode & 0o777,0o600)
            self.assertEqual(output.parent.stat().st_mode & 0o777,0o700)
    def test_journal_raw_messages_are_opt_in(self):
        row={'__REALTIME_TIMESTAMP':'1000000','MESSAGE':'unspecified credential SYNTHETIC_SECRET','PRIORITY':'3'}
        with patch.dict(os.environ,{},clear=True):
            self.assertNotIn('SYNTHETIC_SECRET',record_event(row)['message'])
        with patch.dict(os.environ,{'NEOFAND_INCLUDE_JOURNAL_MESSAGES':'1'}):
            self.assertIn('SYNTHETIC_SECRET',record_event(row)['message'])
    def test_known_credential_redaction(self):
        for message in ['{"token":"SYNTHETIC_SECRET"}','Authorization: Basic SYNTHETIC_SECRET','Authorization: Bearer SYNTHETIC_SECRET']:
            self.assertNotIn('SYNTHETIC_SECRET',scrub(message))

if __name__=='__main__':unittest.main()
