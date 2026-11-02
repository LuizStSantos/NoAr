"""Adversarial checks: incomplete or discontinuous evidence must never pass."""

from copy import deepcopy
import unittest

from verify import connection_metrics, evaluate, packet_metrics


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.packets = {
            kind: {"dts_regressions": 0, "maximum_gap_seconds": gap, "last_dts": 20.0}
            for kind, gap in (("video", 0.04), ("audio", 0.021334))
        }
        self.connections = connection_metrics([{"items": [
            {"id": "publisher", "path": "output", "state": "publish",
             "packetsSendDrop": 0, "packetsReceivedDrop": 0},
            {"id": "reader", "path": "output", "state": "read",
             "packetsSendDrop": 0, "packetsReceivedDrop": 0},
        ]}])
        self.markers = {
            f"{kind}_quarter_seconds": {"a": 8, "b": 8, "live": 8}
            for kind in ("video", "audio")
        }
        self.markers.update({f"{kind}_sequence": ["a", "b", "live", "b"]
                             for kind in ("video", "audio")})
        self.events = [{"event": "start", "cycles": 1}, {"event": "live_open"},
                       {"event": "switch", "source": "live"}, {"event": "live_closed"},
                       {"event": "finished", "status": "PASS"}]

    def checks(self):
        return evaluate(self.packets, self.connections, self.markers, self.events, False)

    def test_complete_evidence_passes(self):
        self.assertEqual(set(self.checks().values()), {"PASS"})

    def test_dts_reversal_is_detected(self):
        probe = {"streams": [{"index": 0, "codec_type": "video"},
                             {"index": 1, "codec_type": "audio"}],
                 "packets": [{"stream_index": index, "dts_time": timestamp}
                             for index in (0, 1) for timestamp in ("1", "2", "1.5")]}
        self.packets = packet_metrics(probe)
        self.assertEqual(self.checks()["video_timestamps"], "FAIL")

    def test_gap_and_av_drift_fail(self):
        self.packets["audio"]["maximum_gap_seconds"] = 0.5
        self.packets["audio"]["last_dts"] = 19
        self.assertEqual(self.checks()["audio_timestamps"], "FAIL")
        self.assertEqual(self.checks()["av_end_alignment"], "FAIL")

    def test_reconnected_publisher_fails(self):
        self.connections["publish"]["count"] = 2
        self.assertEqual(self.checks()["single_output_publisher"], "FAIL")

    def test_video_only_live_is_rejected(self):
        self.markers["audio_quarter_seconds"].pop("live")
        self.markers["audio_sequence"] = ["a", "b"]
        self.assertEqual(self.checks()["three_audio_sources"], "FAIL")
        self.assertEqual(self.checks()["decoded_audio_transitions"], "FAIL")

    def test_packet_drops_or_missing_statistics_fail(self):
        self.connections["read"]["maximum_send_drops"] = 1
        self.assertEqual(self.checks()["no_srt_packet_drops"], "FAIL")
        self.connections["read"]["maximum_send_drops"] = 0
        self.connections["read"]["drop_statistics_available"] = False
        self.assertEqual(self.checks()["no_srt_packet_drops"], "FAIL")

    def test_later_cycle_without_decoded_live_fails(self):
        second = deepcopy(self.events[1:4])
        self.events[0]["cycles"] = 2
        self.events[1:1] = second
        self.assertEqual(self.checks()["all_scheduled_live_entries"], "PASS")
        self.assertEqual(self.checks()["decoded_video_transitions"], "FAIL")

    def test_unscheduled_fallback_fails_normal_run(self):
        self.events.insert(3, {"event": "switch", "source": "b", "reason": "live_stalled"})
        self.assertEqual(self.checks()["expected_live_behavior"], "FAIL")

    def test_unclosed_input_and_missing_completion_fail(self):
        self.events = self.events[:-2]
        self.assertEqual(self.checks()["live_inputs_opened_and_closed"], "FAIL")
        self.assertEqual(self.checks()["demo_completed"], "FAIL")


if __name__ == "__main__":
    unittest.main()
