"""Check Codex repository skills and shared documentation contracts."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
NAMES = (
    "noar-intelligent-development",
    "noar-rust",
    "noar-vue",
    "noar-docker",
    "noar-ffmpeg",
    "noar-video-processing",
)


class CodexCompatibilityTests(unittest.TestCase):
    def test_root_instructions_and_docs_exist(self):
        self.assertTrue((ROOT / "AGENTS.md").is_file())
        self.assertTrue((ROOT / "backend/AGENTS.md").is_file())
        self.assertTrue((ROOT / "docs/ai/CODEX.md").is_file())
        self.assertIn("English", (ROOT / "AGENTS.md").read_text(encoding="utf-8"))

    def test_skill_metadata_and_shared_references(self):
        base = ROOT / ".agents" / "skills"
        self.assertEqual({p.parent.name for p in base.glob("*/SKILL.md")}, set(NAMES))
        for name in NAMES:
            with self.subTest(name=name):
                data = (base / name / "SKILL.md").read_text(encoding="utf-8")
                header = re.match(
                    r"\A---\nname: ([a-z0-9-]+)\ndescription: (.+)\n---\n",
                    data,
                )
                self.assertIsNotNone(header)
                self.assertEqual(header.group(1), name)
                self.assertGreater(len(header.group(2)), 25)
                ref = f".claude/skills/{name}/SKILL.md"
                self.assertIn(ref, data)
                self.assertTrue((ROOT / ref).is_file())
                self.assertLess(len(data), 2600, "Keep Codex adapters thin")


if __name__ == "__main__":
    unittest.main()
