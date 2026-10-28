#!/usr/bin/env python3
"""Find short excerpts of official FFmpeg documentation without bloating LLM context.

Commands (repo root):
    python3 .claude/scripts/ffmpeg_docs.py list
    python3 .claude/scripts/ffmpeg_docs.py topics srt
    python3 .claude/scripts/ffmpeg_docs.py search srt --manual protocols
    python3 .claude/scripts/ffmpeg_docs.py search timestamps --manual ffmpeg
    python3 .claude/scripts/ffmpeg_docs.py capabilities
    python3 .claude/scripts/ffmpeg_docs.py version

Only accesses HTTPS pages listed in docs/ai/ffmpeg/SOURCES.json on official ffmpeg.org.
Network is optional: previously cached manuals can be read offline. No extra pip packages.
This is a reference lookup, NOT a stream tester or a media-operation launcher.
"""
from __future__ import annotations

import argparse
import hashlib
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
from urllib.parse import urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "docs" / "ai" / "ffmpeg" / "SOURCES.json"
DEFAULT_CACHE = ROOT / ".cache" / "noar-ffmpeg-docs"
# Refresh cached docs after one day; nightly online docs may differ from pinned libraries.
CACHE_TTL_SECONDS = 24 * 3600
MAX_DOWNLOAD_BYTES = 16 * 1024 * 1024
MAX_OUTPUT_LINES = 35
MAX_MATCHES = 8
TOPIC_MANUALS = {
    "srt": ["protocols", "formats", "libavformat", "doxygen-modules"],
    "rtmp": ["protocols", "formats", "libavformat"],
    "hls": ["formats", "protocols", "ffmpeg", "libavformat"],
    "udp": ["protocols", "formats"],
    "rtp": ["protocols", "formats"],
    "scte35": ["bitstream-filters", "formats", "libavcodec", "doxygen-modules"],
    "pts": ["ffmpeg", "utils", "libavutil", "doxygen-modules"],
    "dts": ["ffmpeg", "utils", "libavutil", "doxygen-modules"],
    "timestamp": ["ffmpeg", "utils", "libavformat", "doxygen-modules"],
    "sync": ["ffmpeg", "resampler", "scaler", "libavutil"],
    "clock": ["ffmpeg", "utils", "libavutil"],
    "filter": ["filters", "libavfilter", "doxygen-modules"],
    "scale": ["filters", "scaler", "libswscale"],
    "loudness": ["filters", "libswresample"],
    "audio": ["filters", "resampler", "libswresample", "codecs"],
    "video": ["filters", "scaler", "codecs", "libavcodec"],
    "codec": ["codecs", "libavcodec", "doxygen-modules"],
    "encoder": ["codecs", "libavcodec", "ffmpeg"],
    "decoder": ["codecs", "libavcodec", "ffmpeg"],
    "mux": ["formats", "libavformat", "doxygen-modules"],
    "demux": ["formats", "libavformat", "doxygen-modules"],
    "record": ["ffmpeg", "formats", "libavformat"],
    "subtitle": ["filters", "codecs", "formats"],
    "gpu": ["ffmpeg", "codecs", "filters", "devices"],
    "hardware": ["ffmpeg", "codecs", "filters", "devices"],
    "device": ["devices", "libavdevice"],
    "metadata": ["ffmpeg", "formats", "libavformat"],
    "rust": ["doxygen-trunk", "doxygen-modules", "libavcodec", "libavformat"],
}


class TextExtractor(HTMLParser):
    """Minimal HTML text converter; does not execute scripts or HTML."""
    SEPARATORS = frozenset({"p", "li", "h1", "h2", "h3", "h4", "h5", "h6", "div", "section", "pre", "tr", "br", "dt", "dd"})
    HIDDEN = frozenset({"script", "style", "noscript", "svg"})

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.parts: list[str] = []
        self.hidden_depth = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in self.HIDDEN:
            self.hidden_depth += 1
        elif not self.hidden_depth and tag in self.SEPARATORS:
            self.parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag in self.HIDDEN:
            self.hidden_depth = max(0, self.hidden_depth - 1)
        elif not self.hidden_depth and tag in self.SEPARATORS:
            self.parts.append("\n")

    def handle_data(self, data: str) -> None:
        if not self.hidden_depth:
            self.parts.append(data)

    def to_text(self) -> str:
        # Prevent the cached HTML from expanding uncontrollably into line output.
        joined = "".join(self.parts).replace("\xa0", " ")
        lines = [re.sub(r"[ \t]+", " ", ln).strip() for ln in joined.splitlines()]
        return "\n".join(ln for ln in lines if ln)


def validate_official_url(url: str) -> None:
    parsed = urlparse(url)
    if parsed.scheme != "https" or parsed.hostname not in {"ffmpeg.org", "www.ffmpeg.org"}:
        raise ValueError("Only HTTPS official FFmpeg documentation is allowed")
    if parsed.username or parsed.password or parsed.port or parsed.query or parsed.fragment:
        raise ValueError("Credentials, custom ports, query strings and fragments are not supported")
    if not parsed.path.startswith("/") or ".." in parsed.path or not parsed.path.endswith(".html"):
        raise ValueError("Only direct official .html documentation pages are allowed")
    if not (parsed.path.startswith("/doxygen/") or re.fullmatch(r"/[a-zA-Z0-9_-]+\.html", parsed.path)):
        raise ValueError("Page is not an official supported documentation path")


class OfficialOnlyRedirects(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        validate_official_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def registry() -> dict[str, str]:
    data = json.loads(REGISTRY.read_text(encoding="utf-8"))
    manuals = data["manuals"]
    for name, url in manuals.items():
        if not re.fullmatch(r"[a-z][a-z0-9-]*", name):
            raise ValueError(f"Invalid manual identifier: {name}")
        validate_official_url(url)
    return manuals


def load_manual(name: str, manuals: dict[str, str], cache: Path, refresh: bool = False, offline: bool = False) -> tuple[str, str]:
    if name not in manuals:
        raise ValueError(f"Unknown manual '{name}'. Use list.")
    # name is allowlisted and cannot introduce path traversal.
    cache_path = cache / f"{name}.txt"
    if cache_path.exists() and (offline or (not refresh and time.time() - cache_path.stat().st_mtime <= CACHE_TTL_SECONDS)):
        return cache_path.read_text(encoding="utf-8"), "cache"
    if offline:
        raise RuntimeError(f"Offline cache miss: {name}")
    request = Request(manuals[name], headers={"User-Agent": "NoAr-FFmpeg-Documentation-Reference/1.0", "Accept": "text/html"})
    try:
        with build_opener(OfficialOnlyRedirects).open(request, timeout=14) as response:
            validate_official_url(response.geturl())
            raw = response.read(MAX_DOWNLOAD_BYTES + 1)
            if len(raw) > MAX_DOWNLOAD_BYTES:
                raise RuntimeError(f"Official manual exceeded {MAX_DOWNLOAD_BYTES} byte safety limit")
            content_type = response.headers.get("content-type", "")
            if "text/html" not in content_type and "application/xhtml+xml" not in content_type:
                raise RuntimeError(f"Unexpected content type {content_type}")
        extractor = TextExtractor()
        extractor.feed(raw.decode("utf-8", "replace"))
        text = extractor.to_text()
        if not text:
            raise RuntimeError("Manual contains no extractable text")
        cache.mkdir(parents=True, exist_ok=True)
        tmp = cache_path.with_suffix(".tmp")
        tmp.write_text(text, encoding="utf-8")
        tmp.replace(cache_path)
        return text, "fresh-official"
    except Exception as exc:
        if cache_path.exists():
            print(f"WARNING: Network refresh failed ({exc}); using possibly stale cached manual.", file=sys.stderr)
            return cache_path.read_text(encoding="utf-8"), "stale-cache"
        raise RuntimeError(f"Cannot read {name}: {exc}") from exc


def excerpts(document: str, needle: str, context: int, max_matches: int) -> list[tuple[int, str]]:
    lines = document.splitlines()
    found: list[tuple[int, str]] = []
    needle = needle.casefold()
    last_end = -1
    for idx, line in enumerate(lines):
        if needle not in line.casefold():
            continue
        start = max(0, idx - context)
        end = min(len(lines), idx + context + 1)
        if start < last_end:
            start = last_end
        if start >= end:
            continue
        found.append((start + 1, "\n".join(f"{i+1}: {lines[i]}" for i in range(start, end))))
        last_end = end
        if len(found) >= max_matches:
            break
    return found


def run_capabilities() -> int:
    commands = {
        "ffmpeg": ["ffmpeg", "-version"],
        "build": ["ffmpeg", "-buildconf"],
        "protocols": ["ffmpeg", "-protocols"],
        "hwaccels": ["ffmpeg", "-hwaccels"],
        "libraries": ["ffprobe", "-version"],
    }
    # Not a benchmark or stream test; readonly and locally executed.
    for name, command in commands.items():
        print(f"=== {name}: {' '.join(command)} ===")
        try:
            completed = subprocess.run(command, capture_output=True, text=True, timeout=8, check=False)
            out = completed.stdout + ("\n" + completed.stderr if completed.stderr else "")
            # Avoid very large config logs; focus on version and build options.
            print("\n".join(out.splitlines()[:65]))
            if completed.returncode:
                print(f"EXIT_CODE={completed.returncode}")
        except (OSError, subprocess.TimeoutExpired) as exc:
            print(f"NOT_AVAILABLE: {exc}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache-dir", type=Path, default=Path(os.environ.get("NOAR_FFMPEG_CACHE", DEFAULT_CACHE)))
    parser.add_argument("--offline", action="store_true", help="Use only previously cached docs")
    parser.add_argument("--refresh", action="store_true", help="Re-fetch manual from official site")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="Show official manual registry (no network)")
    sub.add_parser("capabilities", help="Show installed FFmpeg version, build, protocols and HW accelerators")
    sub.add_parser("version", help="Print installed ffmpeg and ffprobe versions")
    topic_parser = sub.add_parser("topics", help="Recommend official manuals for a topic, without downloading")
    topic_parser.add_argument("query")
    search_parser = sub.add_parser("search", help="Search one official manual and print only short matching excerpts")
    search_parser.add_argument("query")
    doc_source = search_parser.add_mutually_exclusive_group(required=True)
    doc_source.add_argument("--manual", help="Registry ID from list/topics")
    doc_source.add_argument("--url", help="Exact official FFmpeg HTML page, including versioned Doxygen references")
    search_parser.add_argument("--context", type=int, default=2, help="Context lines around each match (0..8)")
    search_parser.add_argument("--limit", type=int, default=5, help="Maximum matching excerpts (1..8)")
    args = parser.parse_args(argv)
    manuals = registry()
    if args.command == "list":
        for name, url in manuals.items():
            print(f"{name:20s} {url}")
        return 0
    if args.command == "capabilities":
        return run_capabilities()
    if args.command == "version":
        for exe in ("ffmpeg", "ffprobe"):
            try:
                p = subprocess.run([exe, "-version"], capture_output=True, text=True, timeout=8)
                print(f"{exe}: {(p.stdout or p.stderr).splitlines()[0]}")
            except (OSError, subprocess.TimeoutExpired, IndexError) as exc:
                print(f"{exe}: NOT_AVAILABLE ({exc})")
        return 0
    if args.command == "topics":
        query = args.query.casefold()
        # Treat common SCTE-35 spellings as the same topic.
        query = re.sub(r"scte[-_\s]+35", "scte35", query)
        choices: list[str] = []
        for keyword, ids in TOPIC_MANUALS.items():
            if keyword in query:
                choices.extend(ids)
        for name in dict.fromkeys(choices or ["ffmpeg", "doxygen-trunk"]):
            print(f"{name}: {manuals[name]}")
        return 0
    if args.command == "search":
        if not 0 <= args.context <= 8 or not 1 <= args.limit <= MAX_MATCHES:
            parser.error("Context must be 0..8; limit must be 1..8")
        chosen = manuals
        selected_name = args.manual
        if args.url:
            try:
                validate_official_url(args.url)
            except ValueError as exc:
                print(f"INVALID_URL: {exc}", file=sys.stderr)
                return 2
            selected_name = "url-" + hashlib.sha256(args.url.encode("utf-8")).hexdigest()[:24]
            chosen = {selected_name: args.url}
        try:
            text, source = load_manual(selected_name, chosen, args.cache_dir, args.refresh, args.offline)
        except (ValueError, RuntimeError) as exc:
            print(f"NOT_AVAILABLE: {exc}", file=sys.stderr)
            return 2
        print(f"SOURCE: {chosen[selected_name]} | retrieval={source} | search={args.query!r}")
        hits = excerpts(text, args.query, args.context, args.limit)
        if not hits:
            print("NO_MATCH. Try a more specific option name, or another manual via topics.")
            return 1
        for idx, (line, block) in enumerate(hits, 1):
            print(f"\n--- excerpt {idx}, extracted text line {line} ---\n{block}")
        return 0
    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
