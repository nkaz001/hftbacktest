"""Offline regressions for the release-calendar example."""

import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch


path = (
    Path(__file__).resolve().parents[1]
    / "examples/fxmacrodata_release_calendar_filter.py"
)
spec = importlib.util.spec_from_file_location("calendar_filter_sample", path)
sample = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sample)


def event(**updates):
    row = {
        "release_date_confirmed": True,
        "market_tier": 1,
        "announcement_datetime": 1783497600,
        "date": "2026-06-01",
    }
    row.update(updates)
    return row


class CalendarFilterTests(unittest.TestCase):
    def test_confirmed_epoch_and_window_are_preserved(self):
        self.assertEqual(
            {"2026-07-07", "2026-07-08", "2026-07-09"},
            sample.build_blackout_dates([event()], window_days=1),
        )

    def test_invalid_timestamp_does_not_discard_valid_events(self):
        for value in (None, "invalid", float("inf"), True):
            with self.subTest(value=value):
                self.assertEqual(
                    {"2026-07-08"},
                    sample.build_blackout_dates(
                        [
                            event(announcement_datetime=value),
                            event(),
                        ]
                    ),
                )

    def test_invalid_tier_is_skipped_without_misclassification(self):
        for value in (None, "invalid", float("nan"), 1.5, 0, -1, True):
            with self.subTest(value=value):
                self.assertEqual(
                    set(), sample.build_blackout_dates([event(market_tier=value)])
                )

    def test_negative_window_is_rejected_instead_of_disabling_filter(self):
        with self.assertRaises(ValueError):
            sample.build_blackout_dates([event()], window_days=-1)

    def test_window_must_be_an_integer(self):
        for value in (1.5, "1", True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                sample.build_blackout_dates([event()], window_days=value)

    def test_invalid_tier_threshold_is_rejected(self):
        for value in (0, -1, 1.5, "1", True):
            with self.subTest(value=value), self.assertRaises(ValueError):
                sample.build_blackout_dates([event()], min_market_tier=value)

    def test_only_explicit_confirmation_is_accepted(self):
        for value in (False, None, "false", "true", 1):
            with self.subTest(value=value):
                self.assertEqual(
                    set(),
                    sample.build_blackout_dates(
                        [
                            event(release_date_confirmed=value),
                        ]
                    ),
                )

    def test_tier_threshold_and_string_integer_metadata_are_preserved(self):
        self.assertEqual(set(), sample.build_blackout_dates([event(market_tier=2)]))
        self.assertEqual(
            {"2026-07-08"},
            sample.build_blackout_dates(
                [event(market_tier="2")],
                min_market_tier=2,
            ),
        )

    def test_reference_period_is_not_used_as_release_date(self):
        self.assertEqual({"2026-07-08"}, sample.build_blackout_dates([event()]))
        self.assertEqual(
            set(), sample.build_blackout_dates([event(announcement_datetime=None)])
        )

    def test_epoch_zero_remains_valid(self):
        self.assertEqual(
            {"1970-01-01"},
            sample.build_blackout_dates(
                [
                    event(announcement_datetime=0),
                ]
            ),
        )

    def test_date_filter_contract_is_preserved(self):
        self.assertFalse(sample.can_quote(1783497600 * 1_000_000_000, {"2026-07-08"}))
        self.assertTrue(sample.can_quote(1783584000 * 1_000_000_000, {"2026-07-08"}))

    def test_empty_fetch_and_generic_error_are_preserved(self):
        with patch.object(
            sample, "urlopen", return_value=io.StringIO(json.dumps({"data": []}))
        ):
            self.assertEqual(
                [], sample.fetch_release_events("USD", "2026-07-01", "2026-07-31")
            )
        payload = {"detail": "fixture-private-value"}
        with patch.object(
            sample, "urlopen", return_value=io.StringIO(json.dumps(payload))
        ):
            with self.assertRaises(ValueError) as raised:
                sample.fetch_release_events("USD", "2026-07-01", "2026-07-31")
        self.assertNotIn("fixture-private-value", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
