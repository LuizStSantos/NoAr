import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('prepare_release', Path(__file__).with_name('prepare_release.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

CHANGELOG = """# Changelog

## [Unreleased]

### Fixed

- A fix.

## [1.0.0] - 2026-10-21

### Added

- The first release.

[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/LuizStSantos/NoAr/releases/tag/v1.0.0
"""

CARGO = """[workspace]
members = ["backend/app"]

[workspace.package]
version = "1.0.0"
repository = "https://github.com/LuizStSantos/NoAr"

[workspace.dependencies]
serde = { version = "1.0.0" }
"""

LOCK = """[[package]]
name = "noar"
version = "1.0.0"

[[package]]
name = "noar-engine"
version = "1.0.0"

[[package]]
name = "serde"
version = "1.0.0"
"""


class ReleasePlanTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.write(CHANGELOG, CARGO)

    def tearDown(self):
        self.dir.cleanup()

    def write(self, changelog, cargo=CARGO):
        (self.root / 'CHANGELOG.md').write_text(changelog)
        (self.root / 'Cargo.toml').write_text(cargo)
        (self.root / 'Cargo.lock').write_text(LOCK)
        (self.root / 'package.json').write_text(json.dumps({'name': 'frontend', 'version': '1.0.0'}, indent=2) + '\n')
        lock = {'name': 'frontend', 'version': '1.0.0', 'packages': {'': {'name': 'frontend', 'version': '1.0.0'}}}
        (self.root / 'package-lock.json').write_text(json.dumps(lock, indent=2) + '\n')

    def test_fixes_only_make_a_patch(self):
        plan = module.plan(self.root, 'auto', None, 0)
        self.assertEqual((plan['bump'], plan['next']), ('patch', '1.0.1'))

    def test_added_or_a_new_migration_make_a_minor(self):
        self.assertEqual(module.plan(self.root, 'auto', None, 1)['next'], '1.1.0')
        self.write(CHANGELOG.replace('### Fixed', '### Added'))
        self.assertEqual(module.plan(self.root, 'auto', None, 0)['next'], '1.1.0')

    def test_a_breaking_change_makes_a_major(self):
        self.write(CHANGELOG.replace('- A fix.', '- **Breaking**: an API field was renamed.'))
        self.assertEqual(module.plan(self.root, 'auto', None, 0)['next'], '2.0.0')
        self.write(CHANGELOG.replace('### Fixed', '### Removed'))
        self.assertEqual(module.plan(self.root, 'auto', None, 0)['next'], '2.0.0')

    def test_a_chosen_bump_and_a_release_candidate(self):
        plan = module.plan(self.root, 'minor', 'rc.1', 0)
        self.assertEqual(plan['next'], '1.1.0-rc.1')
        self.assertEqual(module.next_version('1.1.0-rc.1', 'minor'), '1.1.0')

    def test_nothing_unreleased_is_refused(self):
        self.write(CHANGELOG.replace('### Fixed\n\n- A fix.\n', ''))
        with self.assertRaises(module.ReleaseError):
            module.plan(self.root, 'auto', None, 0)

    def test_apply_dates_the_notes_and_sets_every_version(self):
        module.apply(self.root, '1.0.0', '1.0.1', '2026-11-02')

        changelog = (self.root / 'CHANGELOG.md').read_text()
        self.assertIn('## [Unreleased]\n\n## [1.0.1] - 2026-11-02\n\n### Fixed\n\n- A fix.', changelog)
        self.assertIn('[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v1.0.1...HEAD\n'
                      '[1.0.1]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...v1.0.1\n'
                      '[1.0.0]:', changelog)
        cargo = (self.root / 'Cargo.toml').read_text()
        self.assertIn('[workspace.package]\nversion = "1.0.1"', cargo)
        # Dependencies keep their own versions.
        self.assertIn('serde = { version = "1.0.0" }', cargo)
        lock = (self.root / 'Cargo.lock').read_text()
        self.assertEqual(lock.count('version = "1.0.1"'), 2)
        self.assertIn('name = "serde"\nversion = "1.0.0"', lock)
        self.assertEqual(json.loads((self.root / 'package.json').read_text())['version'], '1.0.1')
        package_lock = json.loads((self.root / 'package-lock.json').read_text())
        self.assertEqual((package_lock['version'], package_lock['packages']['']['version']), ('1.0.1', '1.0.1'))

    def test_a_version_cannot_be_released_twice(self):
        module.apply(self.root, '1.0.0', '1.0.1', '2026-11-02')
        with self.assertRaises(module.ReleaseError):
            module.apply(self.root, '1.0.1', '1.0.1', '2026-11-03')

    def test_check_writes_nothing(self):
        before = (self.root / 'CHANGELOG.md').read_text()
        self.assertEqual(module.main(['--root', str(self.root), '--check', '--new-migrations', '0']), 0)
        self.assertEqual((self.root / 'CHANGELOG.md').read_text(), before)


if __name__ == '__main__':
    unittest.main()
