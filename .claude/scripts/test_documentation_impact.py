import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('doc_impact', Path(__file__).with_name('analyze_documentation_impact.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class DocumentationImpactTests(unittest.TestCase):
    def test_api_route_maps_to_api_docs(self):
        files = module.impact(['backend/app/src/api/routes/channel.rs'])['documentation_candidates']
        self.assertIn('docs/api.md', [f['path'] for f in files])

    def test_docker_change_suggests_deployment_docs(self):
        docs = [x['path'] for x in module.impact(['docker-compose.yml'])['documentation_candidates']]
        self.assertIn('docs/docker.md', docs)
        self.assertIn('README.md', docs)

    def test_frontend_component_and_map(self):
        docs = [x['path'] for x in module.impact(['frontend/src/components/player/TransportBar.vue'])['documentation_candidates']]
        self.assertIn('docs/ai/frontend.md', docs)
        self.assertIn('docs/ai/repository-map.md', docs)

    def test_map_updates_do_not_trigger_self_review(self):
        self.assertEqual(module.impact(['docs/ai/repository-map.md'])['documentation_candidates'], [])

    def test_deduplicated_references(self):
        result = module.impact(['Dockerfile', 'Dockerfile'])
        self.assertEqual(result['changed_files'], ['Dockerfile'])

    def test_migrations_are_documentation_sensitive(self):
        docs = [x['path'] for x in module.impact(['migrations/00016_something.sql'])['documentation_candidates']]
        self.assertIn('CHANGELOG.md', docs)

if __name__ == '__main__':
    unittest.main()
