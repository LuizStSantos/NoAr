#!/usr/bin/env python3
"""Fill a disposable NoAr server with the demo channel the README screenshots show.

Follow qa/screenshots/README.md to create an owned, fresh loopback server.
Explicit disposable-staging opt-in is required. It completes setup (user
``admin``, password from ``--password-file`` or ``--password``), makes
title-card videos with FFmpeg, writes their guide sheets and artwork, a
commercials folder, a live, an assembled program, marks, today's playlist, a
few scheduled events and the channel logo, then starts the playout.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from zoneinfo import ZoneInfo

FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
ROOT = Path(__file__).resolve().parents[2]
DEMO_TIMEZONE = ZoneInfo("America/Sao_Paulo")

# (folder, file stem, title, subtitle, colors, minutes)
VIDEOS = [
    ("Documentaries/Oceans (2024)", "Oceans", "Oceans", "Documentary · 2024", ("0x0b4f6c", "0x01baef"), 30),
    ("Documentaries/Mountains (2023)", "Mountains", "Mountains", "Documentary · 2023", ("0x3d2c2e", "0xc97b63"), 28),
    ("Series/Kitchen Stories", "Kitchen Stories S01E01", "Kitchen Stories", "S01E01 · Bread", ("0x5c2a00", "0xf4a261"), 22),
    ("Series/Kitchen Stories", "Kitchen Stories S01E02", "Kitchen Stories", "S01E02 · Pasta", ("0x5c2a00", "0xe9c46a"), 22),
    ("Series/Kitchen Stories", "Kitchen Stories S01E03", "Kitchen Stories", "S01E03 · Desserts", ("0x5c2a00", "0xe76f51"), 22),
    ("News/Morning News", "Morning News block 1", "Morning News", "Block 1", ("0x14213d", "0x4361ee"), 9),
    ("News/Morning News", "Morning News block 2", "Morning News", "Block 2", ("0x14213d", "0x4895ef"), 9),
    ("News/Morning News", "Morning News block 3", "Morning News", "Block 3", ("0x14213d", "0x4cc9f0"), 9),
    ("Kids/Planet Kids", "Planet Kids", "Planet Kids", "Animation", ("0x2d6a4f", "0x95d5b2"), 15),
]
SPOTS = [
    ("Spot Coffee", ("0x6f1d1b", "0xbb9457")),
    ("Spot Bank", ("0x03045e", "0x0077b6")),
    ("Spot Car", ("0x212529", "0xadb5bd")),
]


class Api:
    def __init__(self, url: str) -> None:
        self.url = url.rstrip("/")
        self.token = ""

    def call(self, method: str, path: str, body: object = None) -> object:
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.url + path, data=data, method=method)
        request.add_header("Content-Type", "application/json")
        if self.token:
            request.add_header("Authorization", f"Bearer {self.token}")
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with opener.open(request, timeout=120) as response:
                text = response.read().decode()
        except urllib.error.HTTPError as error:
            raise SystemExit(f"{method} {path}: {error.code} {error.read().decode()}") from error
        try:
            return json.loads(text) if text else None
        except json.JSONDecodeError:
            return text


def run(*command: str) -> None:
    subprocess.run(command, check=True, capture_output=True, timeout=120)


def card(target: Path, title: str, subtitle: str, colors: tuple[str, str], size: str) -> None:
    """A still title card: a two-color gradient with the title and subtitle."""
    width, height = (int(value) for value in size.split("x"))
    top, bottom = colors
    big = height // 7 if width >= height else width // 9
    small = big // 2
    run(
        "ffmpeg", "-y", "-v", "error", "-threads", "2", "-filter_threads", "1",
        "-f", "lavfi", "-i", f"gradients=s={size}:c0={top}:c1={bottom}:x0=0:y0=0:x1={width}:y1={height}:d=1",
        "-vf",
        f"drawtext=fontfile={FONT}:text='{title}':fontsize={big}:fontcolor=white:"
        f"x=(w-text_w)/2:y=(h-text_h)/2-{big // 2}:shadowcolor=black@0.5:shadowx=3:shadowy=3,"
        f"drawtext=fontfile={FONT}:text='{subtitle}':fontsize={small}:fontcolor=white@0.85:"
        f"x=(w-text_w)/2:y=(h/2)+{small}",
        "-frames:v", "1", str(target),
    )


def video(target: Path, picture: Path, seconds: int) -> None:
    """A still picture with a soft tone: ten encoded seconds repeated by copy."""
    segment = target.with_suffix(".segment.mp4")
    run(
        "ffmpeg", "-y", "-v", "error",
        "-loop", "1", "-framerate", "25", "-i", str(picture),
        "-f", "lavfi", "-i", "sine=frequency=220:sample_rate=48000",
        "-t", "10", "-c:v", "libx264", "-preset", "ultrafast", "-tune", "stillimage",
        "-pix_fmt", "yuv420p", "-g", "50", "-threads", "2", "-c:a", "aac", "-b:a", "64k", "-af", "volume=0.05",
        "-shortest", str(segment),
    )
    run(
        "ffmpeg", "-y", "-v", "error", "-stream_loop", "-1", "-i", str(segment),
        "-t", str(seconds), "-c", "copy", "-movflags", "+faststart", str(target),
    )
    segment.unlink()


def build_media(storage: Path) -> dict[str, float]:
    lengths: dict[str, float] = {}
    work = storage / ".demo-cards"
    work.mkdir(parents=True, exist_ok=True)

    for folder, stem, title, subtitle, colors, minutes in VIDEOS:
        directory = storage / folder
        directory.mkdir(parents=True, exist_ok=True)
        picture = work / f"{stem}.png"
        card(picture, title, subtitle, colors, "1280x720")
        clip = directory / f"{stem}.mp4"
        if not clip.exists():
            video(clip, picture, minutes * 60)
        lengths[str(clip.relative_to(storage))] = minutes * 60.0
        # Artwork of the program folder (the series folder for episodes).
        program = directory
        if not (program / "poster.jpg").exists():
            label = {"Kitchen Stories": "Series", "Morning News": "News"}.get(title, subtitle.split(" · ")[0])
            card(program / "poster.jpg", title, label, colors, "600x900")
            card(program / "fanart.jpg", title, "", colors, "1280x720")

    commercials = storage / "Commercials"
    commercials.mkdir(exist_ok=True)
    for stem, colors in SPOTS:
        picture = work / f"{stem}.png"
        card(picture, stem.replace("Spot ", ""), "Advertisement", colors, "1280x720")
        clip = commercials / f"{stem}.mp4"
        if not clip.exists():
            video(clip, picture, 30)
        lengths[str(clip.relative_to(storage))] = 30.0

    shutil.rmtree(work)
    assets = storage / "00-assets"
    assets.mkdir(exist_ok=True)
    shutil.copy(ROOT / "assets" / "logo.png", assets / "logo.png")

    live = storage / "Lives" / "City Live"
    live.mkdir(parents=True, exist_ok=True)
    (live / "signal.live").write_text("srt://127.0.0.1:19890?streamid=read:city\n")
    card(live / "poster.jpg", "City Live", "Live", ("0x7b2cbf", "0xe0aaff"), "600x900")

    # A trim, two breaks edited into the programme with the logo off during
    # them, and two L-shape stretches.
    (storage / "Documentaries/Oceans (2024)/Oceans.marks.json").write_text(
        json.dumps(
            {
                "trim": {"from": 4.0, "to": 1796.0},
                "hide": [{"from": 600.0, "to": 720.0}, {"from": 1320.0, "to": 1380.0}],
                "breaks": [{"from": 600.0, "to": 720.0}, {"from": 1320.0, "to": 1380.0}],
                "lshape": [{"from": 300.0, "to": 420.0}, {"from": 1020.0, "to": 1140.0}],
            }
        )
    )
    return lengths


def sheets(api: Api) -> None:
    def save(path: str, kind: str, **fields: object) -> None:
        base = {
            "title": "", "plot": "", "genres": [], "rating": "Livre", "year": "", "premiered": "",
            "countries": [], "credits": [], "season": None, "episode": None, "runtime": None,
        }
        base.update(fields)
        api.call("PUT", "/api/file/1/sheet", {"path": path, "kind": kind, "fields": base})

    save("Documentaries/Oceans (2024)", "movie", title="Oceans", year="2024", genres=["Documentary"],
         plot="A journey through the open sea, from coral reefs to the deep.", countries=["Brazil"],
         credits=[{"name": "Ana Lima", "role": "director"}])
    save("Documentaries/Mountains (2023)", "movie", title="Mountains", year="2023", genres=["Documentary"],
         plot="Life above the clouds, season by season.")
    save("Series/Kitchen Stories", "program", title="Kitchen Stories", genres=["Cooking"],
         plot="Home cooks share the recipes of their families.", year="2025",
         credits=[{"name": "Rita Souza", "role": "actor", "character": "Host"}])
    for number, name in [(1, "Bread"), (2, "Pasta"), (3, "Desserts")]:
        save(f"Series/Kitchen Stories/Kitchen Stories S01E0{number}.mp4", "episode",
             title=name, season=1, episode=number, plot=f"The family recipe of the week: {name.lower()}.")
    save("News/Morning News", "program", title="Morning News", genres=["News"],
         plot="The headlines of the day, live from the studio.")
    save("Kids/Planet Kids", "program", title="Planet Kids", genres=["Kids"], rating="Livre",
         plot="Short stories about the animals of the planet.")
    save("Lives/City Live", "program", title="City Live", genres=["Live"], runtime=60,
         plot="Live from the city square.")


def playlist(api: Api, storage: Path, lengths: dict[str, float], start: str) -> None:
    rotation = [
        "News/Morning News.program",
        "Commercials/Spot Coffee.mp4",
        "Documentaries/Oceans (2024)/Oceans.mp4",
        "Commercials/Spot Bank.mp4",
        "Commercials/Spot Car.mp4",
        "Series/Kitchen Stories/Kitchen Stories S01E01.mp4",
        "Kids/Planet Kids/Planet Kids.mp4",
        "Commercials/Spot Coffee.mp4",
        "Documentaries/Mountains (2023)/Mountains.mp4",
        "Series/Kitchen Stories/Kitchen Stories S01E02.mp4",
        "Commercials/Spot Bank.mp4",
        "Series/Kitchen Stories/Kitchen Stories S01E03.mp4",
    ]
    program_length = 3 * 540.0 + 60.0
    items, total = [], 0.0
    while total < 24 * 3600:
        for source in rotation:
            length = program_length if source.endswith(".program") else lengths[source]
            if source.endswith("Oceans.mp4"):
                entry = {"in": 4.0, "out": 1796.0, "duration": length}
                length = 1792.0
            else:
                entry = {"in": 0.0, "out": length, "duration": length}
            entry["source"] = str(storage / source)
            items.append(entry)
            total += length
            if total >= 24 * 3600:
                break
    today = dt.datetime.now(DEMO_TIMEZONE).date().isoformat()
    api.call("POST", "/api/playlist/1", {"channel": "Example TV", "date": today, "program": items})


def graphics(api: Api) -> dict[str, int]:
    """Author examples through the same model/source endpoints used by Composer."""
    logo = next(item["template"] for item in api.call("GET", "/api/graphics/1/channel")
                if item["kind"] == "logo")
    logo.update(name="Example TV logo", group="Channel", always_on=True)
    logo["layers"][0].update(image="00-assets/logo.png", x=90, y=4, width=8, height=14.222222,
                             opacity=0.9, animation_in="fade", animation_out="fade", animation_ms=400)
    api.call("PUT", f"/api/graphics/1/templates/{logo['id']}", logo)
    api.call("POST", f"/api/graphics/1/channel/{logo['id']}", {"on": True})
    source = api.call("POST", "/api/graphics/1/sources", {
        "name": "Market snapshot", "kind": "csv", "interval": 30,
        "document": "symbol,price,change\nUSD,5.10,+0.35%\nEUR,5.60,-0.12%\n",
    })

    def layer(identifier: str, kind: str, x: float, y: float,
              width: float, height: float, **content: object) -> dict:
        return {
            "id": identifier, "name": identifier.replace("-", " ").title(), "kind": kind,
            "visible": True, "x": x, "y": y, "width": width, "height": height,
            "animation_in": "fade", "animation_out": "fade", "animation_ms": 400,
            **content,
        }

    def model(name: str, group: str, layers: list[dict], always_on: bool = False) -> dict:
        return api.call("POST", "/api/graphics/1/templates", {
            "name": name, "group": group, "duration": 0,
            "during_lshape": "follow", "always_on": always_on, "layers": layers,
        })

    lower_third = model("Presenter", "Programmes", [
        layer("nameplate", "box", 5, 74, 75, 21, fill="#14213d", radius=1.2, shape="rounded"),
        layer("accent", "box", 5, 74, 1, 21, fill="#f28c18"),
        layer("programme-image", "image", 7, 78, 8, 14, image="00-assets/logo.png"),
        layer("presenter-name", "text", 18, 77, 59, 8, text="{name}", font_size=5.5, bold=True),
        layer("presenter-role", "text", 18, 86, 59, 6, text="{role}", font_size=3.4, color="#e7e9ed"),
    ])
    sponsor = model("Sponsor window", "Sponsorship", [
        layer("backdrop", "box", 0, 0, 100, 100, fill="#14213d"),
        layer("sidebar", "box", 0, 0, 28, 100, fill="#f28c18"),
        layer("channel-name", "text", 3, 10, 23, 18, text="EXAMPLE\nTV", font_size=5.5, bold=True),
        layer("sponsor-message", "text", 32, 80, 64, 12, text="Your programme. Our partner.", font_size=4.5),
        layer("video-window", "video", 30, 0, 70, 70),
    ])
    api.call("PUT", f"/api/graphics/1/templates/{sponsor['id']}/assignment", {"role": "lshape"})
    clock = model("Studio clock", "Channel", [
        layer("clock", "clock", 5, 5, 20, 8, font_size=4.5, bold=True,
              fill="#14213d", clock_format="HH:mm:ss", clock_timezone="America/Sao_Paulo"),
    ], always_on=True)
    market = model("Market update", "Information", [
        layer("market-panel", "box", 5, 77, 51, 16, fill="#14213d", radius=1, shape="rounded"),
        layer("quote", "text", 8, 80, 45, 10, text="{symbol}  {price}  {change}", font_size=4.5,
              data_fields={field: {"source_id": source["id"], "column": field, "row": 0}
                           for field in ["symbol", "price", "change"]}),
    ])
    models = {"logo": logo["id"], "presenter": lower_third["id"], "lshape": sponsor["id"],
              "clock": clock["id"], "market": market["id"]}
    for template in api.call("GET", "/api/graphics/1/templates"):
        if template["id"] not in models.values():
            api.call("DELETE", f"/api/graphics/1/templates/{template['id']}")
    return {**models, "source": source["id"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--url", default="http://127.0.0.1:8787")
    parser.add_argument("--storage", type=Path, required=True, help="channel 1 storage folder")
    parser.add_argument("--password", default="Example-2026")
    parser.add_argument("--password-file", type=Path, help="private file containing the disposable password")
    args = parser.parse_args()
    target = urllib.parse.urlparse(args.url)
    if (os.environ.get("NOAR_E2E_STAGING") != "1"
            or os.environ.get("NOAR_E2E_DISPOSABLE") != "1"
            or target.hostname not in {"127.0.0.1", "localhost", "::1"}
            or target.scheme not in {"http", "https"}):
        raise SystemExit("Explicit loopback disposable staging opt-in is required")
    storage = args.storage.resolve()
    if not (storage.parent / ".qa-readme-owned").is_file():
        raise SystemExit("Missing owned README demo marker beside the storage folder")
    password = args.password_file.read_text().strip() if args.password_file else args.password
    api = Api(args.url)

    status = api.call("GET", "/api/setup")
    if not isinstance(status, dict) or not status.get("required"):
        raise SystemExit("Refusing an already initialized server; use a fresh disposable database")
    settings = dict(status["settings"])
    if Path(settings["storage"]).resolve() != storage:
        raise SystemExit("The setup storage does not match the owned demo folder")
    settings.update(username="admin", mail="admin@example.invalid", password=password,
                    two_factor=False, shared=False)
    api.call("POST", "/api/setup", settings)
    login = api.call("POST", "/auth/login", {"username": "admin", "password": password})
    api.token = login["access"]

    channel = api.call("GET", "/api/channel/1")
    if Path(channel["storage"]).resolve() != storage:
        raise SystemExit("The channel storage does not match the owned demo folder")
    channel["name"] = "Example TV"
    channel["timezone"] = "America/Sao_Paulo"
    api.call("PATCH", "/api/channel/1", channel)

    lengths = build_media(storage)
    api.call("PUT", "/api/file/1/commercial", {"path": "Commercials", "commercial": True})
    sheets(api)
    api.call(
        "PUT",
        "/api/schedule/1/programs",
        {
            "path": "News/Morning News.program",
            "title": "Morning News",
            "segments": [
                {"kind": "block", "source": "News/Morning News/Morning News block 1.mp4"},
                {"kind": "break", "clips": ["Commercials/Spot Coffee.mp4", "Commercials/Spot Bank.mp4"]},
                {"kind": "block", "source": "News/Morning News/Morning News block 2.mp4"},
                {"kind": "block", "source": "News/Morning News/Morning News block 3.mp4"},
            ],
        },
    )

    config = api.call("GET", "/api/playout/config/1")
    output = next(item for item in api.call("GET", "/api/playout/outputs/1") if item["name"] == "hls")
    config["output"].update(id=output["id"], mode="hls", width=1280, height=720, fps=25,
                            video_codec="libx264", audio_codec="aac", audio_bitrate=128,
                            hls_segment_duration=2, hls_list_size=12, hls_variants=[])
    config["output"]["video_options"]["preset"] = "ultrafast"
    config["processing"]["mode"] = "playlist"
    config["playlist"].update(day_start="00:00:00", length="24:00:00", infinit=False)
    config["recording"]["enable"] = False
    api.call("PUT", "/api/playout/config/1", config)
    models = graphics(api)

    playlist(api, storage, lengths, config["playlist"]["day_start"])

    # Keep every event ahead of capture time, using the channel's timezone.
    now = dt.datetime.now(DEMO_TIMEZONE)
    today = now.date().isoformat()
    graphic_time = now + dt.timedelta(minutes=25)
    break_time = now + dt.timedelta(minutes=55)
    live_time = now + dt.timedelta(minutes=85)
    live_end = live_time + dt.timedelta(minutes=45)
    for event in [
        {"kind": "media", "sources": ["Commercials/Spot Car.mp4", "Commercials/Spot Coffee.mp4"],
         "title": "Programme break", "start_date": break_time.date().isoformat(), "start_time": break_time.strftime("%H:%M:%S"), "entry_mode": "after_clip",
         "after_mode": "continue", "repeat_days": 127},
        {"kind": "live", "sources": ["Lives/City Live/signal.live"], "title": "City Live",
         "start_date": live_time.date().isoformat(), "start_time": live_time.strftime("%H:%M:%S"), "end_time": live_end.strftime("%H:%M:%S"), "entry_mode": "interrupt",
         "after_mode": "clock", "show_in_guide": True},
    ]:
        api.call("POST", "/api/schedule/1", event)
    api.call("POST", "/api/schedule/1/graphics", {
        "title": "Sponsor window", "start_date": graphic_time.date().isoformat(),
        "start_time": graphic_time.strftime("%H:%M:%S"),
        "end_time": (graphic_time + dt.timedelta(minutes=5)).strftime("%H:%M:%S"),
        "template": {"template_id": models["lshape"], "name": "Sponsor window", "values": {}, "records": {}},
    })

    api.call("POST", "/api/control/1/process", {"command": "start"})
    api.call("POST", "/api/graphics/1/take", {
        "template_id": models["presenter"], "fields": {"name": "Rita Souza", "role": "Host · Example TV"},
        "duration": 0,
    })
    (storage.parent / "demo-manifest.json").write_text(json.dumps({
        "channel": "Example TV", "storage": str(storage), "templates": models, "date": today,
    }, indent=2) + "\n")
    print("Demo channel ready: Example TV, user admin")


if __name__ == "__main__":
    main()
