import unittest
from proxmox_collect import normalize
class ProxmoxTests(unittest.TestCase):
 def test_scope_and_secret_exclusion(self):
  result=normalize('pve-turret',{'cpu':.1,'memory':{'total':100,'used':20},'sensitive':'omit'},[
   {'node':'other','type':'qemu','vmid':101},
   {'node':'pve-turret','type':'lxc','vmid':102,'name':'test','status':'running','config':'omit'}],
   [{'type':'vzdump','id':'102','status':'OK','user':'private','upid':'private'}])
  self.assertEqual([g['vmid'] for g in result['guests']],[102])
  self.assertNotIn('config',result['guests'][0]);self.assertNotIn('sensitive',result['host'])
  self.assertNotIn('user',result['tasks'][0]);self.assertNotIn('upid',result['tasks'][0])
 def test_cpu_uses_cached_node_sample(self):
  result=normalize('nas',{'cpu':0},[{'type':'node','id':'node/nas','cpu':.25}],[])
  self.assertEqual(result['host']['cpu'],.25)
  self.assertIsNone(normalize('nas',{'cpu':0},[],[])['host']['cpu'])
 def test_task_limit(self):
  self.assertEqual(len(normalize('node',{},[],[{}]*60)['tasks']),50)
if __name__=='__main__':unittest.main()
