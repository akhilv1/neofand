import unittest
from proxmox_collect import normalize_smart
class SmartHealthTests(unittest.TestCase):
    def test_nvme_health_and_zero_wear_are_preserved_without_identifiers(self):
        d=normalize_smart({'serial_number':'private','model_name':'NVMe','smart_status':{'passed':True},'temperature':{'current':33},'nvme_smart_health_information_log':{'percentage_used':0,'media_errors':0,'critical_warning':0}})
        self.assertEqual(d['state'],'passed');self.assertEqual(d['endurance_used_pct'],0)
        self.assertNotIn('serial_number',d)
    def test_nonzero_smart_health_exit_is_not_treated_as_command_failure(self):
        self.assertEqual(normalize_smart({'smart_status':{'passed':False}},8)['state'],'failed')
    def test_missing_or_failed_reads_are_unavailable(self):
        self.assertEqual(normalize_smart({})['state'],'unavailable')
        self.assertEqual(normalize_smart({'smart_status':{'passed':True}},2)['state'],'unavailable')
    def test_media_errors_and_critical_warnings_are_visible(self):
        self.assertEqual(normalize_smart({'smart_status':{'passed':True},'nvme_smart_health_information_log':{'media_errors':1}})['state'],'warning')
        self.assertEqual(normalize_smart({'smart_status':{'passed':True},'nvme_smart_health_information_log':{'critical_warning':1}})['state'],'failed')
