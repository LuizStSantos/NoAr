"""Tests for safe, idempotent integration with existing NoAr instructions."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('install_ffmpeg_specialist.py')
spec = importlib.util.spec_from_file_location('install_ffmpeg_specialist', SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class IntegrationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.fixtures = {
            'CLAUDE.md': '# NoAr\n\n## Change safety\n\nDo not deploy.\n',
            '.claude/agents/noar-orchestrator.md': '# Orchestrator\n\n## Cost and mutation boundaries\n\nKeep costs bounded.\n',
            '.claude/skills/noar-intelligent-development/SKILL.md': '# Skill\n\n## 5. NoAr safety and targeted checks\n\nSafety first.\n',
            '.claude/skills/noar-ffmpeg/SKILL.md': '# Existing add-on\n',
            'docs/ai/ffmpeg/SOURCES.json': '{"manuals":{}}\n',
        }
        for name, text in self.fixtures.items():
            p = self.root / name
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding='utf-8')

    def test_dry_run_writes_nothing(self):
        self.assertEqual(module.main(['--root', str(self.root), '--check']), 0)
        for name, content in self.fixtures.items():
            self.assertEqual((self.root / name).read_text(encoding='utf-8'), content)

    def test_apply_idempotent_and_preserves_prior_content(self):
        self.assertEqual(module.main(['--root', str(self.root), '--apply']), 0)
        first = {name: (self.root / name).read_bytes() for name, _, _ in module.CHANGES}
        for name, _, _ in module.CHANGES:
            result = first[name].decode()
            self.assertEqual(result.count(module.MARKER), 1)
            insertion = dict((path, section) for path, _, section in module.CHANGES)[name]
            self.assertEqual(result.replace(insertion, ''), self.fixtures[name])
        self.assertEqual(module.main(['--root', str(self.root), '--apply']), 0)
        for name, _, _ in module.CHANGES:
            self.assertEqual((self.root / name).read_bytes(), first[name])

    def test_missing_anchor_blocks_all_writes(self):
        broken = self.root / '.claude/agents/noar-orchestrator.md'
        broken.write_text('# invalid\n', encoding='utf-8')
        self.assertEqual(module.main(['--root', str(self.root), '--apply']), 2)
        self.assertEqual((self.root / 'CLAUDE.md').read_text(), self.fixtures['CLAUDE.md'])

    def test_missing_skill_blocks(self):
        (self.root / '.claude/skills/noar-ffmpeg/SKILL.md').unlink()
        self.assertEqual(module.main(['--root', str(self.root), '--apply']), 2)
        self.assertEqual((self.root / 'CLAUDE.md').read_text(), self.fixtures['CLAUDE.md'])

if __name__ == '__main__':
    unittest.main()
