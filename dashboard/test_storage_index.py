import os
from pathlib import Path
import tempfile
import unittest
from storage_index import Index


class IndexTests(unittest.TestCase):
    def test_resume_cache_changes_deletes_and_hardlinks(self):
        with tempfile.TemporaryDirectory() as directory,tempfile.TemporaryDirectory() as state:
            p=Path(directory);(p/'a.gguf').write_bytes(b'x'*8192);os.link(p/'a.gguf',p/'b.gguf')
            (p/'photo.jpg').write_bytes(b'x'*4096);(p/'child').mkdir();(p/'child/file.txt').write_text('hello')
            (p/'outside').symlink_to('/etc',target_is_directory=True)
            dev=p.stat().st_dev;fs=f'{os.major(dev)}:{os.minor(dev)}';db=Path(state)/'index.db'
            idx=Index(db);idx.start(fs,directory);idx.process_directory(fs,set());idx.db.close()
            idx=Index(db);self.assertTrue(idx.start(fs,directory))
            while idx.process_directory(fs,set()):pass
            first=idx.finish(fs)
            self.assertEqual(next(c['bytes'] for c in first['categories'] if c['name']=='Models'),8192)
            self.assertFalse(idx.start(fs,directory))
            (p/'photo.jpg').unlink();(p/'child/new.png').write_bytes(b'x'*8192)
            idx.start(fs,directory,force=True)
            while idx.process_directory(fs,set()):pass
            second=idx.finish(fs)
            self.assertEqual(next(c['bytes'] for c in second['categories'] if c['name']=='Photos'),8192)
            progress=idx.progress([fs])['filesystems'][0]
            self.assertGreater(progress['cached_entries_reused'],0);self.assertEqual(progress['state'],'ready');self.assertEqual(progress['folders_pending'],0)
            idx.db.close()
    def test_mount_boundary_excluded(self):
        with tempfile.TemporaryDirectory() as directory,tempfile.TemporaryDirectory() as state:
            p=Path(directory);(p/'mount').mkdir();(p/'mount/weights.gguf').write_bytes(b'x'*10000)
            dev=p.stat().st_dev;fs=f'{os.major(dev)}:{os.minor(dev)}';idx=Index(Path(state)/'index.db');idx.start(fs,directory)
            while idx.process_directory(fs,{str(p/'mount')}):pass
            result=idx.finish(fs);self.assertEqual(result['files_scanned'],0);idx.db.close()

if __name__=='__main__':unittest.main()
