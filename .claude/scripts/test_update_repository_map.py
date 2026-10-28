"""Regression tests for the incremental NoAr symbol index."""
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch
import update_repository_map as indexer


class SymbolIndexerTests(unittest.TestCase):
    def test_large_rust_source_not_cut_at_sixty_symbols(self):
        source = "\n".join(f"pub fn sample_{i}() {{}}" for i in range(125))
        result = indexer.parse_symbols(source, "Rust")
        self.assertEqual(len(result), 125)
        self.assertEqual(result[-1]["name"], "sample_124")

    def test_vue_routing_and_setup_symbols(self):
        source = "const router = createRouter({\nconst app = createApp(App)\nexport const i18n = createI18n({"
        found = [item["name"] for item in indexer.parse_symbols(source, "TypeScript")]
        self.assertEqual(found, ["router", "app", "i18n"])

    def test_schema_changed_to_invalidate_old_parser_cache(self):
        self.assertGreaterEqual(indexer.SCHEMA_VERSION, 2)

    def test_find_refreshes_incompatible_index_first(self):
        import json
        with TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "docs" / "ai"
            output.mkdir(parents=True)
            (output / "symbol-index.json").write_text(
                json.dumps({"schema": 1, "files": {"old.rs": {"symbols": []}}}),
                encoding="utf-8",
            )
            (root / "new.rs").write_text("pub fn new_symbol() {}\n", encoding="utf-8")
            with patch.object(indexer, "ROOT", root), \
                 patch.object(indexer, "OUTPUT_DIR", output), \
                 patch.object(indexer, "INDEX_PATH", output / "symbol-index.json"), \
                 patch.object(indexer, "MAP_PATH", output / "repository-map.md"), \
                 patch.object(indexer, "candidate_files", return_value=["new.rs"]):
                indexer.refresh()
                indexer.find("new_symbol")
                new_index = json.loads((output / "symbol-index.json").read_text())
                self.assertEqual(new_index["schema"], indexer.SCHEMA_VERSION)
                self.assertIn("new.rs", new_index["files"])


if __name__ == "__main__":
    unittest.main()
