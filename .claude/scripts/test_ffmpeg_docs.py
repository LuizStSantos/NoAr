"""Offline unit tests for the bounded, official-only FFmpeg document navigator."""
from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("ffmpeg_docs", HERE / "ffmpeg_docs.py")
assert spec and spec.loader
import sys
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)


class TestFFmpegDocs(unittest.TestCase):
    def test_registry_all_official_https(self):
        reg = module.registry()
        self.assertGreaterEqual(len(reg), 25)
        self.assertIn("protocols", reg)
        self.assertIn("doxygen-trunk", reg)

    def test_reject_untrusted_url(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "sources.json"
            p.write_text(json.dumps({"manuals": {"bad": "https://evil.test/malicious.html"}}))
            with patch.object(module, "REGISTRY", p):
                with self.assertRaises(ValueError):
                    module.registry()

    def test_reject_insecure_url(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "sources.json"
            p.write_text(json.dumps({"manuals": {"bad": "http://www.ffmpeg.org/ffmpeg.html"}}))
            with patch.object(module, "REGISTRY", p):
                with self.assertRaises(ValueError):
                    module.registry()

    def test_reject_redirect_url_with_credentials(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "sources.json"
            p.write_text(json.dumps({"manuals": {"bad": "https://user:pw@www.ffmpeg.org/ffmpeg.html"}}))
            with patch.object(module, "REGISTRY", p):
                with self.assertRaises(ValueError):
                    module.registry()

    def test_unknown_manual_does_not_fetch(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(ValueError):
                module.load_manual("../../etc/passwd", {"ffmpeg": "https://www.ffmpeg.org/ffmpeg.html"}, Path(d))

    def test_offline_miss(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(RuntimeError):
                module.load_manual("ffmpeg", module.registry(), Path(d), offline=True)

    def test_cached_read_offline(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)
            (p / "ffmpeg.txt").write_text("known docs", encoding="utf-8")
            content, source = module.load_manual("ffmpeg", module.registry(), p, offline=True)
            self.assertEqual((content, source), ("known docs", "cache"))

    def test_text_extraction_skips_script(self):
        extractor = module.TextExtractor()
        extractor.feed("<html><h1>FFmpeg</h1><p>SRT latency</p><script>secret()</script></html>")
        result = extractor.to_text()
        self.assertIn("SRT latency", result)
        self.assertNotIn("secret", result)

    def test_bounded_excerpts(self):
        docs = "\n".join(f"line {i} SRT" for i in range(100))
        findings = module.excerpts(docs, "SRT", context=1, max_matches=3)
        self.assertLessEqual(len(findings), 3)
        self.assertLess(sum(len(block) for _, block in findings), 300)

    def test_scte35_topic_spellings(self):
        import contextlib
        import io
        for phrase in ('SCTE-35', 'SCTE 35', 'scte35', 'scte_35'):
            with self.subTest(phrase=phrase), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(module.main(['topics', phrase]), 0)
            self.assertIn('bitstream-filters:', output.getvalue())

    def test_topic_routing(self):
        self.assertEqual(module.TOPIC_MANUALS["srt"][0], "protocols")
        self.assertIn("doxygen-modules", module.TOPIC_MANUALS["pts"])


if __name__ == "__main__":
    unittest.main()

class TestDeepDocumentation(unittest.TestCase):
    def test_doxygen_deep_url(self):
        module.validate_official_url("https://www.ffmpeg.org/doxygen/9.0/group__lavc__core.html")

    def test_deny_redirect_to_other_host(self):
        handler = module.OfficialOnlyRedirects()
        from urllib.request import Request
        with self.assertRaises(ValueError):
            handler.redirect_request(Request("https://www.ffmpeg.org/ffmpeg.html"), None, 302, "Found", {}, "http://127.0.0.1/secret.html")

    def test_nonhtml_url_refused(self):
        with self.assertRaises(ValueError):
            module.validate_official_url("https://www.ffmpeg.org/../../private")
