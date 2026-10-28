"""Tests for NoAr's deterministic changed-path verification selector."""
import unittest
import verify_changes as checks


class VerificationSelectionTests(unittest.TestCase):
    def test_rust_selects_lint_and_test(self):
        selected, risks = checks.commands_for(["backend/app/src/api/auth.rs"])
        self.assertEqual([v["name"] for v in selected], ["rust-format", "rust-test", "rust-clippy"])
        self.assertEqual(risks, [])

    def test_vue_selects_typecheck_and_eslint_and_browser_gate(self):
        selected, risks = checks.commands_for(["frontend/src/App.vue"])
        self.assertEqual(len(selected), 2)
        self.assertTrue(any("browser" in s for s in risks))

    def test_media_requires_staging_not_automatic_streaming(self):
        selected, risks = checks.commands_for(["backend/engine/src/output/encoded.rs"])
        self.assertEqual(len(selected), 3)
        self.assertTrue(any("synthetic-stream" in s for s in risks))

    def test_docs_only_does_not_compile_the_app(self):
        selected, risks = checks.commands_for(["docs/ai/README.md"])
        self.assertEqual([v["name"] for v in selected], ["toolkit-unit-tests"])
        self.assertEqual(risks, [])

    def test_docker_checks_are_read_only(self):
        selected, risks = checks.commands_for(["Dockerfile"])
        self.assertEqual([v["name"] for v in selected], ["docker-compose-config", "dockerfile-build-check"])
        self.assertTrue(risks)

    def test_sql_migration_flags_disposable_db(self):
        selected, risks = checks.commands_for(["migrations/00023_test.sql"])
        self.assertTrue(selected)
        self.assertTrue(any("SQLite migration" in v for v in risks))


if __name__ == "__main__":
    unittest.main()
