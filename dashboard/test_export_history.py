import json,sqlite3,tarfile,tempfile,unittest
from pathlib import Path
from export_history import export
class ExportTests(unittest.TestCase):
 def test_online_wal_backup_and_json(self):
  with tempfile.TemporaryDirectory() as t:
   root=Path(t);src=root/'src';src.mkdir();db=sqlite3.connect(src/'live.sqlite');db.execute('pragma journal_mode=WAL');db.execute('create table readings(n integer)');db.execute('insert into readings values (42)');db.commit()
   (src/'drive-health-history.json').write_text(json.dumps({'events':[{'title':'test'}]}))
   archive=export(src,root/'archives',[src/'live.sqlite'],'test-host')
   with tarfile.open(archive) as tar:
    data=tar.extractfile('test-host/0-live.sqlite').read();(root/'restored.sqlite').write_bytes(data)
    self.assertIn('drive-health-history.json',json.load(tar.extractfile('test-host/manifest.json'))['files'])
   self.assertEqual(sqlite3.connect(root/'restored.sqlite').execute('select n from readings').fetchone(),(42,));db.close()
 def test_missing_mount_refused(self):
  with tempfile.TemporaryDirectory() as t:
   with self.assertRaises(RuntimeError):export(t,t,[],'host',t)
