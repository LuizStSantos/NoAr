#!/usr/bin/env python3
"""Bounded GStreamer experiment, independent of NoAr and its scheduler."""

import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

import gi

gi.require_version("Gst", "1.0")
from gi.repository import GLib, Gst


VIDEO = "video/x-raw,format=I420,width=640,height=360,framerate=25/1"
AUDIO = "audio/x-raw,format=S16LE,rate=48000,channels=1,layout=interleaved"
INPUT = "srt://mediamtx:8890?mode=caller&streamid=read:input"
OUTPUT = "srt://mediamtx:8890?mode=caller&streamid=publish:output"
WARMUP = 5
STEP = 6


def element(factory, name=None, **properties):
    item = Gst.ElementFactory.make(factory, name)
    if item is None:
        raise RuntimeError(f"Missing GStreamer element: {factory}")
    for key, value in properties.items():
        item.set_property(key.replace("_", "-"), value)
    return item


def chain(*items):
    for left, right in zip(items, items[1:]):
        if not left.link(right):
            raise RuntimeError(f"Cannot link {left.name} to {right.name}")


class Source:
    """Decode one source to normalized raw A/V on the shared pipeline clock."""

    def __init__(self, name, pipeline, mixers, log, live=False):
        self.name = name
        self.pipeline = pipeline
        self.mixers = mixers
        self.log = log
        self.bin = Gst.Bin.new(name)
        self.pads = {}
        self.last = {"video": 0.0, "audio": 0.0}
        self.first = set()
        self.live = live
        self.decoder = element("decodebin" if live else "uridecodebin")
        self.decoder.connect("pad-added", self.on_pad)
        self.bin.add(self.decoder)
        if live:
            network = element("srtsrc", "live-srt", uri=INPUT, latency=120,
                              auto_reconnect=False)
            self.bin.add(network)
            chain(network, self.decoder)
        else:
            self.decoder.set_property("uri", Path(f"/fixtures/{name}.mp4").as_uri())

        self.queues = {}
        for kind, factories, caps in (
            ("video", ("videoconvert", "videoscale", "videorate"), VIDEO),
            ("audio", ("audioconvert", "audioresample"), AUDIO),
        ):
            queue = element("queue", max_size_buffers=0, max_size_bytes=0,
                            max_size_time=Gst.SECOND)
            items = [queue] + [element(factory) for factory in factories]
            if kind == "video":
                # A newly attached live stream already carries pipeline running time.
                # Do not synthesize pictures from zero before its first real frame.
                next(item for item in items if item.get_factory().get_name() == "videorate").set_property("skip-to-first", True)
            items.append(element("capsfilter", caps=Gst.Caps.from_string(caps)))
            for item in items:
                self.bin.add(item)
            chain(*items)
            self.queues[kind] = queue
            ghost = Gst.GhostPad.new(kind, items[-1].get_static_pad("src"))
            self.bin.add_pad(ghost)
            ghost.add_probe(Gst.PadProbeType.BUFFER, self.observe, kind)

        pipeline.add(self.bin)
        for kind, mixer in mixers.items():
            pad = mixer.request_pad_simple("sink_%u")
            if kind == "video":
                pad.set_property("alpha", 0.0)
            else:
                pad.set_property("mute", True)
            self.pads[kind] = pad
            ghost = self.bin.get_static_pad(kind)
            if ghost.link(pad) != Gst.PadLinkReturn.OK:
                raise RuntimeError(f"Cannot attach {name} {kind}")
        self.bin.sync_state_with_parent()

    def on_pad(self, decoder, pad):
        caps = pad.get_current_caps() or pad.query_caps(None)
        media = caps.to_string().split("/", 1)[0]
        queue = self.queues.get(media)
        if queue is not None and not queue.get_static_pad("sink").is_linked():
            result = pad.link(queue.get_static_pad("sink"))
            if result != Gst.PadLinkReturn.OK:
                self.log("source_link_error", source=self.name, media=media,
                         result=str(result))

    def observe(self, pad, info, kind):
        self.last[kind] = time.monotonic()
        if kind not in self.first:
            self.first.add(kind)
            buffer = info.get_buffer()
            segment_event = pad.get_sticky_event(Gst.EventType.SEGMENT, 0)
            segment = segment_event.parse_segment() if segment_event else None
            self.log("source_buffer", source=self.name, media=kind,
                     pts=buffer.pts / Gst.SECOND, offset=pad.get_offset() / Gst.SECOND,
                     segment_start=segment.start / Gst.SECOND if segment else None,
                     segment_base=segment.base / Gst.SECOND if segment else None)
        return Gst.PadProbeReturn.OK

    def ready(self):
        now = time.monotonic()
        return all(now - timestamp < 0.75 for timestamp in self.last.values())

    def close(self):
        self.bin.set_state(Gst.State.NULL)
        for kind, pad in self.pads.items():
            self.bin.get_static_pad(kind).unlink(pad)
            self.mixers[kind].release_request_pad(pad)
        self.pipeline.remove(self.bin)


class Demo:
    def __init__(self, cycles):
        self.started = time.monotonic()
        self.events = Path("/artifacts/events.jsonl").open("w", buffering=1)
        self.loop = GLib.MainLoop()
        self.pipeline = Gst.Pipeline.new("prototype")
        self.sources = {}
        self.retired_live = []
        self.active = None
        self.live_requested = False
        self.failed = False
        self.output_counts = {"video": 0, "audio": 0}
        # Both aggregators keep producing on the clock if an input stalls.
        # Reserve latency before adding live ingest to avoid changing it on air.
        self.mixers = {
            "video": Gst.parse_launch(
                "compositor name=video-mixer force-live=true background=black "
                "ignore-inactive-pads=true min-upstream-latency=250000000 latency=100000000"),
            "audio": Gst.parse_launch(
                "audiomixer name=audio-mixer force-live=true "
                "ignore-inactive-pads=true min-upstream-latency=250000000 latency=100000000"),
        }
        for mixer in self.mixers.values():
            self.pipeline.add(mixer)
        output = Gst.parse_bin_from_description(
            "identity name=video-time ! " + VIDEO +
            " ! queue ! x264enc tune=zerolatency speed-preset=ultrafast bitrate=1200 "
            "key-int-max=25 bframes=0 ! h264parse ! queue ! mux. "
            "identity name=audio-time ! " + AUDIO +
            " ! queue ! audioconvert ! avenc_aac bitrate=96000 ! aacparse ! queue ! mux. "
            "mpegtsmux name=mux alignment=7 ! queue ! "
            f'srtsink name=output uri="{OUTPUT}" latency=120 sync=true '
            "async=false auto-reconnect=false", False,
        )
        for kind in self.mixers:
            target = output.get_by_name(f"{kind}-time")
            output.add_pad(Gst.GhostPad.new(kind, target.get_static_pad("sink")))
            target.get_static_pad("src").add_probe(
                Gst.PadProbeType.BUFFER, self.observe_output, kind)
        self.pipeline.add(output)
        for kind, mixer in self.mixers.items():
            if mixer.get_static_pad("src").link(output.get_static_pad(kind)) != Gst.PadLinkReturn.OK:
                raise RuntimeError(f"Cannot attach output {kind}")

        bus = self.pipeline.get_bus()
        bus.add_signal_watch()
        bus.connect("message", self.on_message)
        for name in ("a", "b"):
            self.sources[name] = Source(name, self.pipeline, self.mixers, self.log)
        self.switch("a", "startup")
        self.cycles = cycles
        self.duration = WARMUP + cycles * STEP * 4
        self.log("start", gstreamer=Gst.version_string(), cycles=cycles,
                 duration=self.duration, file_mode="clock", output_reconnect=False)

    def log(self, event, **fields):
        record = {"event": event, "elapsed": round(time.monotonic() - self.started, 3),
                  **fields}
        line = json.dumps(record)
        self.events.write(line + "\n")
        print(line, flush=True)

    def observe_output(self, pad, info, kind):
        self.output_counts[kind] += 1
        return Gst.PadProbeReturn.OK

    def switch(self, name, reason):
        source = self.sources.get(name)
        if source is None or (name == "live" and not source.ready()):
            self.log("switch_unavailable", source=name, reason=reason)
            return
        for candidate in self.sources.values():
            candidate.pads["video"].set_property("alpha", 1.0 if candidate is source else 0.0)
            candidate.pads["audio"].set_property("mute", candidate is not source)
        self.active = name
        self.log("switch", source=name, reason=reason)

    def open_live(self):
        self.log("live_open")
        self.sources["live"] = Source("live", self.pipeline, self.mixers, self.log, live=True)

    def enter_live(self):
        self.live_requested = True
        self.switch("live", "scheduled_live")

    def leave_live(self):
        self.live_requested = False
        self.switch("b", "scheduled_return")
        self.close_live()

    def close_live(self):
        source = self.sources.pop("live", None)
        if source:
            source.close()
            # Keep ownership of queued bus messages after the bin is detached.
            self.retired_live.append(source)
            self.log("live_closed")

    def watchdog(self):
        live = self.sources.get("live")
        if self.active == "live" and (live is None or not live.ready()):
            self.switch("b", "live_stalled")
        elif self.live_requested and self.active != "live" and live and live.ready():
            self.switch("live", "live_ready")
        return True

    def cancel(self):
        self.failed = True
        self.log("cancelled")
        self.loop.quit()

    def on_message(self, bus, message):
        if message.type == Gst.MessageType.ERROR:
            error, debug = message.parse_error()
            live = self.sources.get("live")
            if live and (message.src == live.bin or message.src.has_as_ancestor(live.bin)):
                self.log("live_error", message=str(error))
                self.switch("b", "live_error")
                self.close_live()
            elif any(message.src == source.bin or message.src.has_as_ancestor(source.bin)
                     for source in self.retired_live):
                self.log("live_teardown_error", message=str(error))
            else:
                self.log("error", element=message.src.get_path_string(), message=str(error),
                         debug=debug)
                self.failed = True
                self.loop.quit()
        elif message.type == Gst.MessageType.EOS:
            self.log("error", message="Unexpected pipeline EOS before the demo completed")
            self.failed = True
            self.loop.quit()
        elif message.type == Gst.MessageType.LATENCY:
            self.pipeline.recalculate_latency()

    def schedule(self, seconds, callback, *args):
        def invoke():
            callback(*args)
            return False
        GLib.timeout_add(int(seconds * 1000), invoke)

    def run(self):
        try:
            if self.pipeline.set_state(Gst.State.PLAYING) == Gst.StateChangeReturn.FAILURE:
                raise RuntimeError("Cannot start the prototype pipeline")
            for cycle in range(self.cycles):
                base = WARMUP + cycle * STEP * 4
                self.schedule(base, self.switch, "a", "playlist_a")
                self.schedule(base + STEP, self.switch, "b", "playlist_b")
                self.schedule(base + STEP + 2, self.open_live)
                self.schedule(base + STEP * 2, self.enter_live)
                self.schedule(base + STEP * 3, self.leave_live)
            self.schedule(self.duration, self.loop.quit)
            GLib.timeout_add(100, self.watchdog)
            self.loop.run()
        finally:
            self.pipeline.set_state(Gst.State.NULL)
        self.log("finished", status="FAIL" if self.failed else "PASS",
                 output_buffers=self.output_counts)
        self.events.close()
        return int(self.failed)


def run_source():
    pipeline = Gst.parse_launch(
        "videotestsrc is-live=true pattern=ball background-color=0xff009000 "
        "! " + VIDEO + " ! x264enc tune=zerolatency speed-preset=ultrafast "
        "bitrate=1200 key-int-max=25 bframes=0 ! h264parse ! queue ! mux. "
        "audiotestsrc is-live=true freq=880 volume=0.2 ! " + AUDIO +
        " ! audioconvert ! avenc_aac bitrate=96000 ! aacparse ! queue ! mux. "
        "mpegtsmux name=mux alignment=7 ! "
        'srtsink uri="srt://mediamtx:8890?mode=caller&streamid=publish:input" '
        "latency=120 sync=true async=false auto-reconnect=false"
    )
    loop = GLib.MainLoop()
    failed = False

    def message(bus, item):
        nonlocal failed
        if item.type == Gst.MessageType.ERROR:
            print(item.parse_error(), file=sys.stderr, flush=True)
            failed = True
            loop.quit()

    bus = pipeline.get_bus()
    bus.add_signal_watch()
    bus.connect("message", message)
    signal.signal(signal.SIGTERM, lambda *_: loop.quit())
    signal.signal(signal.SIGINT, lambda *_: loop.quit())
    try:
        pipeline.set_state(Gst.State.PLAYING)
        loop.run()
    finally:
        pipeline.set_state(Gst.State.NULL)
    return int(failed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("source", "demo"))
    parser.add_argument("--cycles", type=int, choices=range(1, 6), default=3)
    args = parser.parse_args()
    if os.environ.get("NOAR_GST_STAGING") != "1":
        parser.error("Set NOAR_GST_STAGING=1 only for this disposable Compose project")
    Gst.init(None)
    if args.mode == "source":
        return run_source()
    demo = Demo(args.cycles)
    signal.signal(signal.SIGTERM, lambda *_: demo.cancel())
    signal.signal(signal.SIGINT, lambda *_: demo.cancel())
    return demo.run()


if __name__ == "__main__":
    sys.exit(main())
