import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("slylogic", ROOT / "logic/worker.py")
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)


def request(**options):
    data = w.defaults()
    data.update(options)
    mw, _ = w.reconstruct(data)
    return {"version": 1, "revision": 1,
            "identity": {"game": w.GAME, "seed": "test", "team": 0, "slot": 1, "save": "test-save"},
            "connected": True, "history_ready": True, "slot_data": data,
            "received_items": [], "checked_locations": [],
            "missing_locations": [x.address for x in mw.get_locations(1) if x.address is not None],
            "game_facts": {"observed_events": []}}


def receive(p, name, count=1):
    code = w.item_table[name].ap_code
    for _ in range(count):
        p["received_items"].append({"index": len(p["received_items"]), "item_id": code})


class LogicTests(unittest.TestCase):
    def evaluate(self, p):
        out = w.Worker().evaluate(p)
        self.assertEqual(out["status"], "known", out["reason"])
        return out

    def test_keys_open_actual_region_rules(self):
        p = request()
        receive(p, "Tide of Terror: Episode Unlock")
        out = self.evaluate(p)
        self.assertFalse(out["bk"])
        by_id = {x["id"]: x for x in out["checks"]}
        self.assertEqual(by_id[10020102]["status"], "unavailable")
        receive(p, "Tide of Terror: Key")
        out = self.evaluate(p)
        self.assertEqual({x["id"]: x for x in out["checks"]}[10020102]["status"], "available")

    def test_bk_all_reachable_checks_then_new_key(self):
        p = request()
        receive(p, "Tide of Terror: Episode Unlock")
        reachable = {x["id"] for x in self.evaluate(p)["checks"] if x["status"] == "available"}
        p["checked_locations"] = sorted(reachable)
        p["missing_locations"] = [x for x in p["missing_locations"] if x not in reachable]
        self.assertTrue(self.evaluate(p)["bk"])
        receive(p, "Tide of Terror: Key")
        self.assertFalse(self.evaluate(p)["bk"])

    def test_boss_reachable_does_not_mean_defeated(self):
        p = request(RequiredBosses=1)
        receive(p, "Tide of Terror: Episode Unlock")
        receive(p, "Tide of Terror: Key", 7)
        self.assertFalse(self.evaluate(p)["goal"]["clockwerk_accessible"])
        p["checked_locations"] = [10020229]
        p["missing_locations"].remove(10020229)
        self.assertTrue(self.evaluate(p)["goal"]["clockwerk_accessible"])

    def test_individual_bottles_and_minigame_exclusion(self):
        p = request(LocationCluesanityBundleSize=1, IncludeHourglasses=1,
                    ExcludeMinigames=["Crabs", "Races", "Turrets", "Hover Blasters", "Chicken Killing", "Swamp Skiff"])
        out = self.evaluate(p)
        self.assertIn(10020400, p["missing_locations"])
        self.assertIn(10020301, p["missing_locations"])
        self.assertNotIn(10021000, p["missing_locations"])
        self.assertGreater(out["total_count"], 600)

    def test_vault_depends_on_received_bottles_not_collected_checks(self):
        p = request(LocationCluesanityBundleSize=1, ItemCluesanityBundleSize=1)
        receive(p, "Tide of Terror: Episode Unlock")
        def vault():
            return next(x for x in self.evaluate(p)["checks"] if x["id"] == 10020201)
        self.assertEqual(vault()["status"], "unavailable")
        receive(p, "Stealthy Approach: Bottle(s)", 20)
        self.assertEqual(vault()["status"], "available")

    def test_slot_and_identity_fail_closed(self):
        for mutation in (lambda p: p["identity"].update(game="other"),
                         lambda p: p["slot_data"].update(Seed="wrong"),
                         lambda p: p.update(history_ready=False),
                         lambda p: p["slot_data"].pop("RequiredBosses"),
                         lambda p: p["slot_data"].update(RequiredBosses=500),
                         lambda p: p["missing_locations"].append(42)):
            p = request()
            mutation(p)
            out = w.Worker().evaluate(p)
            self.assertIsNone(out["bk"])
            self.assertEqual(out["status"], "unknown")

    def test_stale_revision_and_item_gap_rejected(self):
        p = request()
        worker = w.Worker()
        self.assertEqual(worker.evaluate(p)["status"], "known")
        self.assertEqual(worker.evaluate(p)["status"], "unknown")
        p["revision"] += 1
        p["received_items"] = [{"index": 1, "item_id": 10020001}]
        self.assertEqual(worker.evaluate(p)["status"], "unknown")

    def test_key_cache_reconstruction_from_session_id(self):
        p = request()
        p["missing_locations"].append(10029000)
        self.assertIn(10029000, [x["id"] for x in self.evaluate(p)["checks"]])

    def test_goal_suppresses_bk_and_input_is_immutable(self):
        p = request()
        p["game_facts"]["observed_events"] = ["Victory"]
        before = copy.deepcopy(p)
        out = self.evaluate(p)
        self.assertTrue(out["goal_complete"])
        self.assertFalse(out["bk"])
        self.assertEqual(p, before)
        self.assertNotIn("commands", out)

    def test_catalog_original_fields(self):
        cat = w.catalog()
        self.assertEqual(len(cat["slot_fields"]), 20)
        self.assertEqual(len({x["id"] for x in cat["items"]}), len(cat["items"]))
        self.assertEqual(len({x["id"] for x in cat["locations"]}), len(cat["locations"]))


if __name__ == "__main__":
    unittest.main()
