import unittest
from proxmox_collect import normalize_filesystems
class GuestCapacityTests(unittest.TestCase):
    def test_mirrored_volume_counted_once_and_dsm_root_excluded(self):
        volume={'name':'dm-2','mountpoint':'/volume2','disk':[{},{}],'type':'btrfs','total-bytes':16000,'used-bytes':11000}
        root=dict(volume,name='md0',mountpoint='/',**{'total-bytes':8,'used-bytes':2})
        rows=normalize_filesystems([volume,dict(volume),root])
        self.assertEqual(len(rows),1)
        self.assertEqual(rows[0]['used_bytes'],11000)
        self.assertEqual(rows[0]['total_bytes'],16000)
    def test_invalid_and_pseudo_filesystems_excluded(self):
        valid={'name':'sda','mountpoint':'/','disk':[{}],'total-bytes':100,'used-bytes':40}
        rows=normalize_filesystems([valid,dict(valid,name='loop',mountpoint='/tmp/test'),dict(valid,name='bad',**{'used-bytes':200})])
        self.assertEqual(len(rows),1)
        self.assertEqual(rows[0]['mountpoint'],'/')
