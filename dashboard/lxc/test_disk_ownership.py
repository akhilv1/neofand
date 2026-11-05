import unittest
from types import SimpleNamespace
from unittest.mock import patch
from proxmox_collect import guest_storage

class DiskOwnershipTests(unittest.TestCase):
    def test_only_complete_physical_disks_get_vm_ownership(self):
        def run(args, **kwargs):
            if args[0] == 'qm':
                return SimpleNamespace(stdout='scsi0: /dev/testdisk\nscsi1: /dev/testpart\nscsi2: /dev/testlogical\nscsi3: local-zfs:vm-100-disk-0\nargs: -drive file=/dev/testdisk,format=raw\n')
            self.assertEqual(args[:3], ['lsblk', '-ndo', 'TYPE'])
            return SimpleNamespace(stdout={'/dev/testdisk':'disk\n','/dev/testpart':'part\n','/dev/testlogical':'lvm\n'}[args[-1]])
        with patch('proxmox_collect.subprocess.run', side_effect=run):
            uses=guest_storage([{'vmid':100,'type':'qemu','name':'VM'}, {'vmid':300,'type':'lxc','name':'CT'}])
        self.assertEqual(len(uses),1)
        self.assertEqual(uses[0]['target'],'/dev/testdisk')
        self.assertTrue(uses[0]['whole_disk'])
        self.assertEqual(uses[0]['relation'],'passthrough')

if __name__=='__main__': unittest.main()
