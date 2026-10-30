"""Protocol checks against a published cue and malformed staging references."""

import base64
from pathlib import Path
import tempfile
import unittest

from integrated_checks import HlsObserver, crc32_mpeg2, sections


# The public time_signal example from threefive.Cue.get documentation.
REFERENCE = '/DAvAAAAAAAA///wBQb+dGKQoAAZAhdDVUVJSAAAjn+fCAgAAAAALKChijUCAKnMZ1g='


def packet(pid, payload, start):
    data = bytearray(b'\xff' * 188)
    data[:4] = bytes([0x47, (pid >> 8) | (0x40 if start else 0), pid & 255, 0x30])
    data[4] = 183 - len(payload)
    data[5] = 0
    data[5 + data[4]:] = payload
    return bytes(data)


class ProtocolChecks(unittest.TestCase):
    def test_crc_matches_the_mpeg2_standard_check_value(self):
        self.assertEqual(crc32_mpeg2(b'123456789'), 0x0376E6E7)

    def test_published_cue_reassembles_across_ts_packets(self):
        cue = base64.b64decode(REFERENCE)
        self.assertEqual(crc32_mpeg2(cue), 0)
        packets = [packet(500, b'\x00' + cue[:20], True), packet(500, cue[20:], False)]
        self.assertEqual([data for _, data in sections(packets, 500)], [cue])
        self.assertEqual(list(sections(packets, 501)), [])

    def test_hls_references_cannot_leave_the_test_origin_or_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            observer = HlsObserver('http://127.0.0.1:19787/public/1/live/', Path(directory)/'hls')
            for uri in ['https://example.invalid/segment.ts', '../private.ts', '/api/setup']:
                with self.subTest(uri=uri), self.assertRaises(ValueError):
                    observer.fetch(uri)

    def test_independent_decoder_detects_an_incorrect_command_length(self):
        try:
            from threefive import Cue
        except ImportError:
            self.skipTest('Install requirements.txt for the independent SCTE-35 decoder')
        cue = Cue(REFERENCE)
        self.assertEqual(cue.info_section.splice_command_length, cue.command.command_length)
        malformed = bytearray(base64.b64decode(REFERENCE))
        malformed[12] += 1
        malformed[-4:] = crc32_mpeg2(malformed[:-4]).to_bytes(4, 'big')
        with self.assertRaises(ValueError):
            Cue(bytes(malformed))


if __name__ == '__main__':
    unittest.main()
