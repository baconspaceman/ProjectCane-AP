"""Read-only Sly rule evaluator. No AP connection, grant path, or game-save writes."""
from __future__ import annotations

import argparse
import copy
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT / "runtime" / "ap-core"
sys.path.insert(0, str(CORE))
# The staged core only contains Sly and generic. Never discover official custom worlds.
import Utils
Utils.user_path.cached_path = str(CORE)
from BaseClasses import MultiWorld, CollectionState, ItemClassification
from Options import OptionSet, Choice, Range, Toggle
from worlds.sly1 import Sly1World
from worlds.sly1.Items import item_table
from worlds.sly1.Options import Sly1Options
from worlds.sly1.NativeOptions import LINK_FIELDS
from worlds.sly1.Types import Sly1Item, Sly1Location
from worlds.sly1.Locations import KEY_CACHE_BASE, KEY_LOCATION_NAMES, sly_locations
from worlds.sly1.data.Constants import LEVELS, BOSSES, MOVES, MOVE_NAMES
from worlds.sly1.Locations import bottle_amounts

GAME = Sly1World.game
FIELDS = tuple(name for name in Sly1Options.__annotations__ if name not in LINK_FIELDS)
ITEM_NAMES = {v.ap_code: k for k, v in item_table.items() if v.ap_code is not None}
BOSS_EVENTS = {10020229: "Beat Raleigh", 10020230: "Beat Muggshot",
               10020231: "Beat Mz. Ruby", 10020232: "Beat Panda King", 10020233: "Victory"}
MAX_BYTES = 8 * 1024 * 1024


def defaults():
    return {name: copy.deepcopy(Sly1Options.type_hints[name].default) for name in FIELDS}


def parse_options(data):
    missing = set(FIELDS) - set(data)
    if missing:
        raise ValueError("Missing slot fields: " + ", ".join(sorted(missing)))
    values = {}
    for name, cls in Sly1Options.type_hints.items():
        value = copy.deepcopy(data[name] if name in FIELDS else cls.default)
        if name in FIELDS:
            if issubclass(cls, OptionSet):
                if not isinstance(value, (list, set, frozenset)) or not all(isinstance(x, str) for x in value):
                    raise ValueError(f"{name}: expected a list of option names")
                if set(value) - set(cls.valid_keys):
                    raise ValueError(f"{name}: unsupported option names")
            elif isinstance(value, bool):
                value = int(value)
            elif not isinstance(value, int):
                raise ValueError(f"{name}: expected integer slot encoding")
            if issubclass(cls, Toggle) and value not in (0, 1):
                raise ValueError(f"{name}: invalid toggle")
            if issubclass(cls, Choice) and value not in cls.name_lookup:
                raise ValueError(f"{name}: invalid choice")
            if issubclass(cls, Range) and not cls.range_start <= value <= cls.range_end:
                raise ValueError(f"{name}: outside allowed range")
        values[name] = cls.from_any(value)
    return Sly1Options(**values)


def reconstruct(slot_data, location_ids=None):
    """Construct actual region/rule graph without fill, random itempool or event sweeping.

    UT's generate_early only restores options but calls global random for ALL+AvoidEarlyBK.
    Apply the same option fields directly, retaining upstream create_regions/set_rules.
    Starting inventory must come from complete AP ReceivedItems, never guessed here.
    """
    mw = MultiWorld(1)
    mw.set_seed(0, name="read-only-logic")
    mw.game[1] = GAME
    mw.player_name = {1: "Logic"}
    world = Sly1World(mw, 1)
    mw.worlds[1] = world
    world.options = parse_options(slot_data)
    world.create_regions()
    if location_ids is not None:
        # Original slot_data omits generated key-cache count. Recover ONLY actual session IDs.
        for code in sorted(location_ids):
            offset = code - KEY_CACHE_BASE
            if 0 <= offset < len(KEY_LOCATION_NAMES) * 10:
                name = KEY_LOCATION_NAMES[offset // 10]
                reg = mw.get_region(sly_locations[name].region, 1)
                reg.locations.append(Sly1Location(1, f"{name} Cache #{offset % 10 + 1}", code, reg))
    world.set_rules()
    return mw, world


def catalog():
    fields = []
    for name in FIELDS:
        cls = Sly1Options.type_hints[name]
        fields.append({"name": name, "type": cls.__bases__[0].__name__,
                       "default": sorted(cls.default) if isinstance(cls.default, (set, frozenset)) else cls.default,
                       "choices": getattr(cls, "options", None),
                       "valid_keys": sorted(cls.valid_keys) if issubclass(cls, OptionSet) else None,
                       "min": getattr(cls, "range_start", None), "max": getattr(cls, "range_end", None)})
    scenes = [{"world": world, "level": level, "area": name.replace("The Swamp's", "Swamp's")}
              for world, names in enumerate(LEVELS.values(), 1) for level, name in enumerate(names)]
    scenes.extend({"world": world, "level": 8, "area": BOSSES[world-1][0]} for world in range(1, 5))
    scenes.extend([{"world": 0, "level": 3, "area": "Paris"}, {"world": 0, "level": 4, "area": "Hideout"}])
    scenes.extend({"world": 5, "level": level, "area": "Cold Heart of Hate"} for level in (0, 2, 3, 4, 5, 6, 8))
    native_fields = [{"name": name, "type": "Toggle", "default": 0} for name in LINK_FIELDS]
    effects = []
    for index, name in enumerate(MOVE_NAMES):
        masks = MOVES[name]
        effects.append({"id": 10020001 + index, "masks": masks if isinstance(masks, list) else [masks]})
    bottle_effects = []
    for item_id in range(10020030, 10020049):
        level = ITEM_NAMES[item_id].removesuffix(": Bottle(s)")
        for world, names in enumerate(LEVELS.values(), 1):
            if level in names:
                bottle_effects.append({"id": item_id, "world": world, "level": names.index(level),
                                       "maximum": bottle_amounts[level].bottle_amount})
    return {"game": GAME, "world_release": "v0.3.5-alpha-hotfix1", "slot_fields": fields, "scenes": scenes,
            "native_slot_fields": native_fields, "native_bridge_contract": "sly1-native-v1",
            "move_effects": effects, "bottle_effects": bottle_effects,
            "items": [{"id": d.ap_code, "name": n, "classification": int(d.classification),
                       "count": d.count, "kind": "Repeatable" if n in ("Charm", "1-Up") or "Trap" in n else "Permanent"}
                      for n, d in item_table.items() if d.ap_code is not None],
            "locations": [{"id": code, "name": name} for name, code in Sly1World.location_name_to_id.items() if code is not None]}


class Worker:
    def __init__(self):
        self.revisions = {}

    def evaluate(self, payload):
        out = {"version": 1, "revision": payload.get("revision"), "identity": payload.get("identity"),
               "status": "unknown", "reason": "", "bk": None, "checks": []}
        try:
            identity = payload["identity"]
            if payload.get("version") != 1 or identity.get("game") != GAME:
                raise ValueError("Unsupported protocol or game")
            if not all(identity.get(k) for k in ("seed", "save")):
                raise ValueError("Missing seed/save identity")
            if type(identity.get("slot")) is not int or identity["slot"] < 1 or type(identity.get("team")) is not int or identity["team"] < 0:
                raise ValueError("Invalid team/slot identity")
            revision = payload["revision"]
            if type(revision) is not int or revision < 0:
                raise ValueError("Invalid revision")
            key = json.dumps(identity, sort_keys=True)
            if revision <= self.revisions.get(key, -1):
                raise ValueError("Stale or duplicate revision")
            self.revisions[key] = revision
            if payload.get("connected") is not True or payload.get("history_ready") is not True:
                raise ValueError("Connection/history not ready")
            data = payload["slot_data"]
            if data.get("Seed") is not None and str(data["Seed"]) != str(identity["seed"]):
                raise ValueError("Slot-data seed mismatch")
            checked = set(payload["checked_locations"])
            missing = set(payload["missing_locations"])
            if checked & missing:
                raise ValueError("Checked/missing lists overlap")
            ids = checked | missing
            if not ids or not all(type(x) is int for x in ids):
                raise ValueError("Missing or malformed session location set")
            mw, world = reconstruct(data, ids)
            locations = {x.address: x for x in mw.get_locations(1) if x.address is not None}
            if set(locations) != ids:
                raise ValueError("Session location set does not match pinned world/options")
            state = CollectionState(mw)
            received = payload["received_items"]
            for index, record in enumerate(received):
                if record.get("index") != index or record.get("item_id") not in ITEM_NAMES:
                    raise ValueError("Incomplete indexed item history or unsupported item ID")
                name = ITEM_NAMES[record["item_id"]]
                # Track even useful Roll: its classification changes with YAML. Actual rules decide relevance.
                state.collect(Sly1Item(name, ItemClassification.progression, record["item_id"], 1), prevent_sweep=True)
            facts = payload.get("game_facts", {})
            events = set(facts.get("observed_events", []))
            if events - set(BOSS_EVENTS.values()):
                raise ValueError("Unknown local event")
            events.update(name for code, name in BOSS_EVENTS.items() if code in checked)
            for name in events:
                state.collect(Sly1Item(name, ItemClassification.progression, None, 1), prevent_sweep=True)
            available = []
            for code, location in sorted(locations.items()):
                reachable = location.can_reach(state)
                status = "completed" if code in checked else "available" if reachable else "unavailable"
                if status == "available":
                    available.append(code)
                area = location.name.split(":", 1)[0] if ":" in location.name else location.name
                if code == 10020000:
                    area = "Paris"
                if code == 10020233:
                    area = "Cold Heart of Hate"
                out["checks"].append({"id": code, "name": location.name, "area": area, "logic_region": location.parent_region.name,
                                      "status": status, "requirements": "" if status != "unavailable" else
                                      "Unavailable in current logic; detailed requirements not available."})
            out.update(status="known", goal_complete="Victory" in events,
                       bk=False if "Victory" in events else not available,
                       available_count=len(available), checked_count=len(checked), total_count=len(ids),
                       goal={"bosses_defeated": len(events - {"Victory"}),
                             "required_bosses": world.options.RequiredBosses.value,
                             "pages_received": state.count("Thievius Raccoonus Page", 1),
                             "required_pages": world.options.RequiredPages.value,
                             "clockwerk_accessible": mw.get_region("Cold Heart of Hate", 1).can_reach(state)})
        except (KeyError, ValueError, TypeError, AttributeError, AssertionError) as exc:
            out.update(status="unknown", reason=str(exc), bk=None, checks=[])
        return out


def atomic_json(path, data):
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    os.replace(tmp, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bridge-dir", type=Path)
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--catalog", type=Path)
    args = parser.parse_args()
    if args.catalog:
        atomic_json(args.catalog, catalog())
        return
    if not args.bridge_dir:
        parser.error("--bridge-dir is required")
    worker = Worker()
    last = None
    source = args.bridge_dir / "logic-input.json"
    target = args.bridge_dir / "logic-output.json"
    while True:
        try:
            if source.stat().st_size > MAX_BYTES:
                raise ValueError("Logic input exceeds size limit")
            raw = source.read_bytes()
            if raw != last:
                payload = json.loads(raw)
                if not isinstance(payload, dict):
                    raise ValueError("Expected object")
                atomic_json(target, worker.evaluate(payload))
                last = raw
        except FileNotFoundError:
            if args.once:
                raise
        except (ValueError, OSError) as exc:
            atomic_json(target, {"version": 1, "status": "unknown", "bk": None, "checks": [], "reason": str(exc)})
        if args.once:
            return
        time.sleep(0.25)


if __name__ == "__main__":
    main()
