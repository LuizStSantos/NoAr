"""Regression tests for the incremental NoAr symbol index."""
import unittest
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


if __name__ == "__main__":
    unittest.main()
