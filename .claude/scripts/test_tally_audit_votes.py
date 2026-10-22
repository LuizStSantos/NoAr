import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('audit_votes', Path(__file__).with_name('tally_audit_votes.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class CouncilVoteTests(unittest.TestCase):
    def setUp(self):
        self.findings = {'findings': [{'id':'F-001','title':'Example issue','severity':'HIGH',
            'verification':'VERIFIED_STATIC','is_blocker':True,'evidence':['frontend/src/App.vue:10']}]}
        self.ballots = {name:{'agent':name,
                 'readiness':{'vote':'approve','reason':'Reviewed findings'},
                 'findings':{'F-001':{'vote':'dismiss','reason':'Not reproduced'}}}
                 for name in module.VOTERS}

    def test_approve_with_three_yes_and_no_blocker(self):
        for name in list(module.VOTERS)[3:]:
            self.ballots[name]['readiness']['vote']='deny'
        result=module.tally(self.findings,self.ballots)
        self.assertEqual(result['readiness'],'APPROVED_ADVISORY_ONLY')
        self.assertEqual(result['findings'][0]['decision'],'DISMISSED')

    def test_three_denials_reject(self):
        for name in list(module.VOTERS)[:3]:
            self.ballots[name]['readiness']['vote']='deny'
        self.assertEqual(module.tally(self.findings,self.ballots)['readiness'],'DENIED')

    def test_confirmed_critical_or_high_blocker_blocks_approval(self):
        for name in list(module.VOTERS)[:3]:
            self.ballots[name]['findings']['F-001']['vote']='confirm'
        self.assertEqual(module.tally(self.findings,self.ballots)['readiness'],'BLOCKED')

    def test_two_two_one_is_inconclusive(self):
        for name in list(module.VOTERS)[:2]:
            self.ballots[name]['readiness']['vote']='deny'
            self.ballots[name]['findings']['F-001']['vote']='confirm'
        self.ballots[module.VOTERS[2]]['readiness']['vote']='abstain'
        self.ballots[module.VOTERS[2]]['findings']['F-001']['vote']='abstain'
        result=module.tally(self.findings,self.ballots)
        self.assertEqual(result['readiness'],'INCONCLUSIVE')
        self.assertEqual(result['findings'][0]['decision'],'UNRESOLVED')

    def test_reject_missing_ballot(self):
        self.ballots.pop(module.VOTERS[-1])
        with self.assertRaises(ValueError): module.tally(self.findings,self.ballots)

    def test_reject_missing_issue_vote(self):
        self.ballots[module.VOTERS[0]]['findings']={}
        with self.assertRaises(ValueError): module.tally(self.findings,self.ballots)

class BackendCouncilTests(unittest.TestCase):
    def setUp(self):
        self.findings = {'council': 'backend', 'findings': [{
            'id': 'BE-001', 'title': 'Backend example', 'severity': 'CRITICAL',
            'verification': 'VERIFIED_STATIC', 'is_blocker': True,
            'evidence': ['backend/engine/src/playout.rs:50']}]}
        self.ballots = {name: {'agent':name,
            'readiness':{'vote':'approve','reason':'Independently reviewed evidence'},
            'findings':{'BE-001': {'vote':'dismiss','reason':'Evidence did not support finding'}}}
            for name in module.BACKEND_VOTERS}

    def test_backend_three_confirmed_blockers(self):
        for agent in module.BACKEND_VOTERS[:3]:
            self.ballots[agent]['findings']['BE-001']['vote']='confirm'
        result=module.tally(self.findings,self.ballots,'backend')
        self.assertEqual(result['council'],'backend')
        self.assertEqual(result['readiness'],'BLOCKED')
        self.assertEqual(result['findings'][0]['decision'],'CONFIRMED')

    def test_backend_three_deny(self):
        for agent in module.BACKEND_VOTERS[:3]:
            self.ballots[agent]['readiness']['vote']='deny'
        result=module.tally(self.findings,self.ballots,'backend')
        self.assertEqual(result['readiness'],'DENIED')

    def test_mixed_councils_rejected(self):
        self.ballots.pop(module.BACKEND_VOTERS[0])
        self.ballots[module.FRONTEND_VOTERS[2]]={'agent':module.FRONTEND_VOTERS[2],
          'readiness':{'vote':'approve','reason':'Test'},
          'findings':{'BE-001':{'vote':'dismiss','reason':'Test'}}}
        with self.assertRaises(ValueError): module.tally(self.findings,self.ballots,'backend')

    def test_findings_council_mismatch_rejected(self):
        with self.assertRaises(ValueError): module.tally(self.findings,self.ballots,'frontend')

if __name__ == '__main__': unittest.main()
