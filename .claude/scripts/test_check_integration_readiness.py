import copy
import unittest
from check_integration_readiness import GATES, check


def make(status='NOT_RUN'):
    return {
        'schema': 1, 'repository': 'LuizStSantos/NoAr', 'commit': 'abcdef1234567',
        'environment': 'staging',
        'gates': {n: {'status': status, 'reason': 'Observed result' if status in ('PASS','FAIL') else 'Not executed',
                      'evidence': ['test-output.txt'] if status in ('PASS','FAIL') else []} for n in GATES}
    }

class IntegrationGateTests(unittest.TestCase):
    def test_all_pass(self):
        self.assertEqual(check(make('PASS'))['decision'], 'READY_FOR_REVIEW')
    def test_all_missing(self):
        self.assertEqual(check(make())['decision'], 'NEEDS_TESTING')
    def test_failure_blocks(self):
        data=make('PASS'); data['gates']['scheduler']['status']='FAIL'
        self.assertEqual(check(data)['decision'], 'NOT_READY')
    def test_unavailable_needs_testing(self):
        data=make('PASS'); data['gates']['media_output']={'status':'NOT_AVAILABLE','reason':'No FFmpeg fixture','evidence':[]}
        self.assertEqual(check(data)['decision'], 'NEEDS_TESTING')
    def test_no_forged_pass_without_evidence(self):
        data=make('PASS'); data['gates']['public_api']['evidence']=[]
        with self.assertRaises(ValueError): check(data)
    def test_unknown_gate_rejected(self):
        data=make(); data['gates']['unknown']={'status':'NOT_RUN','reason':'None','evidence':[]}
        with self.assertRaises(ValueError): check(data)
    def test_production_rejected(self):
        data=make(); data['environment']='production'
        with self.assertRaises(ValueError): check(data)
    def test_malformed_status(self):
        data=make(); data['gates']['security']['status']='MAYBE'
        with self.assertRaises(ValueError): check(data)

if __name__ == '__main__': unittest.main()
