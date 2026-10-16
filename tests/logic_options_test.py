"""Bounded actual-rule checks for native parity review; never fill or generate a seed."""
import unittest
from logic_worker_test import w, request, receive


def evaluate(payload):
    result = w.Worker().evaluate(payload)
    if result["status"] != "known":
        raise AssertionError(result["reason"])
    return result


class OptionParityTests(unittest.TestCase):
    def test_page_access_does_not_require_boss_defeat(self):
        p = request(UnlockClockwerk=2, RequiredPages=2, MaxPages=2)
        receive(p, "Thievius Raccoonus Page")
        self.assertFalse(evaluate(p)["goal"]["clockwerk_accessible"])
        receive(p, "Thievius Raccoonus Page")
        result = evaluate(p)
        self.assertTrue(result["goal"]["clockwerk_accessible"])
        self.assertFalse(result["goal_complete"])

    def test_roll_rule_uses_actual_received_roll(self):
        p = request(IncludeHourglasses=1, HourglassesRequireRoll=1)
        receive(p, "Tide of Terror: Episode Unlock")
        receive(p, "Tide of Terror: Key")
        def status():
            return next(x["status"] for x in evaluate(p)["checks"] if x["id"] == 10020301)
        self.assertEqual(status(), "unavailable")
        receive(p, "Progressive Roll")
        self.assertEqual(status(), "available")

    def test_raleigh_trick_does_not_waive_second_gate(self):
        p = request(EnableTricks=["tot_raleigh_early"])
        receive(p, "Tide of Terror: Episode Unlock")
        receive(p, "Tide of Terror: Key")
        def status():
            return next(x["status"] for x in evaluate(p)["checks"] if x["id"] == 10020229)
        self.assertEqual(status(), "unavailable")
        receive(p, "Tide of Terror: Key", 2)
        self.assertEqual(status(), "available")

    def test_all_start_requires_server_start_inventory(self):
        p = request(StartingEpisode=6)
        before = evaluate(p)
        self.assertFalse(any(x["status"] == "available" and x["id"] == 10020122 for x in before["checks"]))
        for episode in ("Tide of Terror", "Sunset Snake Eyes", "Vicious Voodoo", "Fire in the Sky"):
            receive(p, episode + ": Episode Unlock")
        after = evaluate(p)
        self.assertTrue(any(x["status"] == "available" and x["id"] == 10020122 for x in after["checks"]))

    def test_bundle_ids_are_thresholds_not_dense_bundle_indices(self):
        p = request(LocationCluesanityBundleSize=3)
        ids = set(p["missing_locations"])
        self.assertTrue({10020402, 10020405, 10020417, 10020419} <= ids)
        self.assertFalse({10020400, 10020401, 10020403} & ids)

    def test_minigame_caches_only_for_included_minigames(self):
        p = request(MinigameCaches=2, ExcludeMinigames=["Crabs"])
        ids = set(p["missing_locations"])
        self.assertNotIn(10021000, ids)
        self.assertNotIn(10021001, ids)
        self.assertTrue({10021100, 10021101, 10021102} <= ids)


if __name__ == "__main__":
    unittest.main()
