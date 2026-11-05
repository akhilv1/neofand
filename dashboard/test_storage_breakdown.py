import importlib.util
import os
from pathlib import Path
import tempfile
import time
import unittest

spec=importlib.util.spec_from_file_location('breakdown',Path(__file__).with_name('storage_breakdown.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class BreakdownTests(unittest.TestCase):
    def test_classification_precedence(self):
        cases={'/mnt/models/blobs/123abc':'Models','/home/neo/.cache/huggingface/hub/blobs/xyz':'Models','/mnt/backups/photo.jpg':'Backups','/home/neo/photo.heic':'Photos','/home/neo/movie.mkv':'Videos','/home/neo/song.flac':'Audio','/home/neo/a.gguf':'Models','/home/neo/a.zip':'Archives','/home/neo/a.bin':'Other','/usr/share/icons/a.png':'System','/var/lib/docker/data':'Containers','/home/neo/.venvs/package/a.py':'Apps & Developer'}
        for name,expected in cases.items():self.assertEqual(m.category(name),expected)
    def test_no_follow_dedup_and_mount_boundary(self):
        with tempfile.TemporaryDirectory() as tmp:
            p=Path(tmp);(p/'model.gguf').write_bytes(b'x'*8192)
            os.link(p/'model.gguf',p/'hardlink.gguf')
            (p/'external').symlink_to('/etc',target_is_directory=True)
            (p/'skip').mkdir();(p/'skip/huge.gguf').write_bytes(b'x'*65536)
            dev=p.stat().st_dev;fsid=f'{os.major(dev)}:{os.minor(dev)}'
            data=m.scan(tmp,fsid,{str(p/'skip')},time.monotonic()+5)
            models=next(c for c in data['categories'] if c['name']=='Models')
            self.assertEqual(models['bytes'],(p/'model.gguf').stat().st_blocks*512)
            self.assertTrue(data['complete']);self.assertEqual(data['files_scanned'],2)
            with self.assertRaises(OSError):m.scan(tmp,'0:999',set(),time.monotonic()+5)
    def test_deadline_marks_partial(self):
        with tempfile.TemporaryDirectory() as tmp:
            dev=os.stat(tmp).st_dev
            self.assertFalse(m.scan(tmp,f'{os.major(dev)}:{os.minor(dev)}',set(),0)['complete'])

if __name__=='__main__':unittest.main()
