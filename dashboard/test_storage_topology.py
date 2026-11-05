import copy
import unittest
from unittest.mock import patch
from storage_topology import enrich, parse_mountinfo, parse_zpool_status, parse_zfs_list, discover_zfs


def node(name,ident,kind='part',children=None,mounts=None,usage=None,fstype='ext4'):
    return dict(name=name,kname=name,device='/dev/'+name,filesystem_id=ident,type=kind,children=children or [],
                mounts=mounts or [],usage=usage,fstype=fstype,auxiliary=False,size=1000,roles=[])


def used(n=100):return dict(total_bytes=1000,used_bytes=n,available_bytes=900,reserved_bytes=0)

STATUS='''  pool: rpool
 state: DEGRADED
  scan: resilver in progress
config:

        NAME        STATE     READ WRITE CKSUM
        rpool       DEGRADED     0     0     0
          mirror-0  DEGRADED     0     0     0
            /dev/sda2 ONLINE     0     0     0
            /dev/sdb2 OFFLINE    0     0     0
        cache
          /dev/sdc1 ONLINE       0     0     0
        spares
          /dev/sdd1 AVAIL
errors: No known data errors
'''

class TopologyTests(unittest.TestCase):
    def test_raid_root_unique_and_roles(self):
        root=node('md0','9:0','raid1',mounts=['/'],usage=used())
        efi=node('sda1','8:1',mounts=['/boot/efi'],usage=used(10));efi['auxiliary']=True
        s=dict(drives=[node('sda','8:0','disk',[efi,node('sda2','8:2',children=[root])]),node('sdb','8:16','disk',[node('sdb2','8:18',children=[copy.deepcopy(root)])])])
        result=enrich(s,[],[],'not-installed')
        self.assertEqual(len(result['volumes']),1)
        self.assertEqual(result['totals']['used_bytes'],100)
        self.assertEqual(result['drives'][0]['roles'],['Boot','System'])
        self.assertEqual(result['drives'][1]['roles'],['System'])
        self.assertEqual(result['volumes'][0]['backing_drives'],['/dev/sda','/dev/sdb'])
        self.assertNotIn('md0',[n['name'] for n in result['drives'][0]['display_nodes']])
    def test_lvm_over_raid_not_duplicated(self):
        lv=node('vg-root','253:0','lvm',mounts=['/'],usage=used(250))
        md=node('md0','9:0','raid1',[lv],fstype='LVM2_member')
        s=dict(drives=[node('sda','8:0','disk',[node('sda1','8:1',children=[md])]),node('sdb','8:16','disk',[node('sdb1','8:17',children=[copy.deepcopy(md)])])])
        result=enrich(s,[],[],'not-installed');self.assertEqual(len(result['volumes']),2)
        self.assertEqual(result['totals']['used_bytes'],250)
        self.assertTrue(all('System' in d['roles'] for d in result['drives']))
    def test_partition_on_raid_is_logical(self):
        part=node('md0p1','259:10',mounts=['/'],usage=used())
        s=dict(drives=[node('sda','8:0','disk',[node('md0','9:0','raid1',[part])])])
        result=enrich(s,[],[],'not-installed');self.assertEqual(len(result['volumes']),2)
        self.assertNotIn('md0p1',[n['name'] for n in result['drives'][0]['display_nodes']])
    def test_zfs_mirror_root_space_and_cache_roles(self):
        mounts=parse_mountinfo('42 1 0:42 / / rw - zfs rpool/ROOT/linux rw')
        ds=parse_zfs_list('rpool\tfilesystem\t300\t700\t10\t10\t0\t/rpool\tno\nrpool/ROOT/linux\tfilesystem\t290\t700\t280\t280\t10\t/\tyes',mounts)
        status=parse_zpool_status(STATUS)['rpool'];pool=dict(id='zfs:rpool',name='rpool',datasets=ds,accounting=dict(used_bytes=300,available_bytes=700),**status)
        drives=[node('sd'+l,'8:'+str(i*16),'disk',[node('sd'+l+suffix,'8:'+str(i*16+int(suffix)),fstype='zfs_member')]) for i,(l,suffix) in enumerate([('a','2'),('b','2'),('c','1'),('d','1')])]
        result=enrich(dict(drives=drives),mounts,[pool],'available')
        self.assertEqual(result['totals'],dict(used_bytes=300,available_bytes=700))
        self.assertEqual(pool['backing_drives'],['/dev/sda','/dev/sdb'])
        self.assertEqual(result['drives'][2]['roles'],[])
        self.assertEqual(result['drives'][0]['roles'],['System'])
        self.assertEqual([m['role'] for m in pool['members']],['data','data','cache','spares'])
        self.assertEqual(len(result['scan_targets']),1)
    def test_missing_zfs_permissions(self):
        with patch('storage_topology.shutil.which',return_value='/usr/sbin/zfs'):
            pools,status=discover_zfs([],runner=lambda args:(_ for _ in ()).throw(PermissionError()))
            self.assertEqual(pools,[]);self.assertEqual(status,'unavailable')
    def test_mount_space_escape(self):
        self.assertEqual(parse_mountinfo(r'1 0 8:1 / /mnt/a\040b rw - ext4 /dev/sda1 rw')[0]['path'],'/mnt/a b')

if __name__=='__main__':unittest.main()
