#!/usr/bin/env python3
"""Capture original SRT payload without a demuxer or a transport-stream remux."""

import ctypes
import ctypes.util
import json
from pathlib import Path
import signal
import sys
import threading


def main():
    path, uri = Path(sys.argv[1]).resolve(), sys.argv[2]
    if not uri.startswith('srt://127.0.0.1:29911?mode=listener'):
        raise SystemExit('Only the disposable loopback SRT listener is supported')
    gst = ctypes.CDLL(ctypes.util.find_library('gstreamer-1.0'))
    gst.gst_init.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    gst.gst_parse_launch.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
    gst.gst_parse_launch.restype = ctypes.c_void_p
    gst.gst_element_set_state.argtypes = [ctypes.c_void_p, ctypes.c_int]
    gst.gst_element_set_state.restype = ctypes.c_int
    gst.gst_element_get_bus.argtypes = [ctypes.c_void_p]
    gst.gst_element_get_bus.restype = ctypes.c_void_p
    gst.gst_bus_timed_pop_filtered.argtypes = [ctypes.c_void_p, ctypes.c_uint64, ctypes.c_int]
    gst.gst_bus_timed_pop_filtered.restype = ctypes.c_void_p
    gst.gst_object_unref.argtypes = [ctypes.c_void_p]
    gst.gst_mini_object_unref.argtypes = [ctypes.c_void_p]
    gst.gst_init(None, None)
    error = ctypes.c_void_p()
    pipeline = gst.gst_parse_launch(
        f'srtsrc uri={json.dumps(uri)} ! filesink location={json.dumps(str(path))} sync=false'.encode(),
        ctypes.byref(error))
    if error.value or not pipeline:
        raise SystemExit('Cannot create native SRT capture pipeline; inspect installed srtsrc/filesink plugins')
    stop = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stop.set())
    signal.signal(signal.SIGINT, lambda *_: stop.set())
    bus = gst.gst_element_get_bus(pipeline)
    try:
        if gst.gst_element_set_state(pipeline, 4) == 0:
            raise RuntimeError('Native SRT capture failed to start')
        while not stop.is_set():
            message = gst.gst_bus_timed_pop_filtered(bus, 100_000_000, 3)
            if message:
                gst.gst_mini_object_unref(message)
                raise RuntimeError('Native SRT capture received EOS or a pipeline error')
    finally:
        gst.gst_element_set_state(pipeline, 1)
        gst.gst_object_unref(bus)
        gst.gst_object_unref(pipeline)


if __name__ == '__main__':
    main()
