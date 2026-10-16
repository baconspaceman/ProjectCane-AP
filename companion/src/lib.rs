//! Sly-specific policy and read-only UI/logic projections over the shared SDK.
//! This crate never applies native rewards itself.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use universal_ap_bridge::{Profile, SessionIdentity};

pub const GAME: &str = "Sly Cooper and the Thievius Raccoonus";
pub const CONTRACT: &str = "sly1-native-v1";

pub fn validate_slot(profile: &Profile, data: &Value) -> Result<(), String> {
    if profile.game != GAME || profile.contract != CONTRACT {
        return Err("unsupported Sly game or native contract".into());
    }
    if !data.is_object() {
        return Err("Sly slot data must be an object".into());
    }
    for (key, low, high) in [
        ("UnlockClockwerk", 1, 2),
        ("RequiredBosses", 1, 4),
        ("MaxPages", 1, 50),
        ("RequiredPages", 1, 50),
        ("StartingEpisode", 1, 6),
        ("FastClockwerk", 0, 1),
        ("IncludeHourglasses", 0, 1),
        ("HourglassesRequireRoll", 0, 1),
        ("AvoidEarlyBK", 0, 1),
        ("CutsceneSkip", 0, 1),
        ("LocationCluesanityBundleSize", 0, 5),
        ("ItemCluesanityBundleSize", 0, 5),
        ("MinigameCaches", 0, 10),
        ("TrapChance", 0, 100),
        ("IcePhysicsTrapWeight", 0, 100),
        ("SpeedChangeTrapWeight", 0, 100),
        ("InvisibilityTrapWeight", 0, 100),
        ("BallTrapWeight", 0, 100),
    ] {
        let value = data
            .get(key)
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("missing integer Sly option {key}"))?;
        if value < low || value > high || (key == "StartingEpisode" && value == 5) {
            return Err(format!("unsupported value for Sly option {key}"));
        }
    }
    for (key, allowed) in [
        (
            "ExcludeMinigames",
            &[
                "Crabs",
                "Races",
                "Turrets",
                "Hover Blasters",
                "Chicken Killing",
                "Swamp Skiff",
            ][..],
        ),
        (
            "EnableTricks",
            &[
                "tot_second_half_early",
                "tot_raleigh_early",
                "sse_second_half_early",
                "sse_muggshot_early",
                "vv_second_half_early",
                "fits_second_half_early",
                "fits_panda_early",
                "unseen_foe_invis_skip",
            ][..],
        ),
    ] {
        let values = data
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("missing array Sly option {key}"))?;
        if values
            .iter()
            .any(|v| !v.as_str().is_some_and(|s| allowed.contains(&s)))
        {
            return Err(format!("unsupported entry in {key}"));
        }
    }
    if data
        .get("Seed")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err("Sly slot data has no Seed".into());
    }
    if let Some(version) = data.get("bridge_version")
        && (version.as_u64() != Some(1)
            || data.get("bridge_contract").and_then(Value::as_str) != Some(CONTRACT))
    {
        return Err("incompatible additive Sly bridge contract".into());
    }
    Ok(())
}

/// A control request is consumed before execution and never serialized back.
/// Passwords are runtime-only; no Debug implementation deliberately.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub version: u32,
    pub id: String,
    pub save: String,
    pub action: String,
    pub endpoint: Option<String>,
    pub slot_name: Option<String>,
    pub password: Option<String>,
}
impl Control {
    pub fn validate(&self, save: &str) -> Result<(), String> {
        if self.version != 1 || self.id.is_empty() || self.id.len() > 128 || self.save != save {
            return Err("control version/id/save mismatch".into());
        }
        if !["connect", "disconnect", "reconnect"].contains(&self.action.as_str()) {
            return Err("unsupported control action".into());
        }
        if self.password.as_ref().is_some_and(|s| s.len() > 1024)
            || self.slot_name.as_ref().is_some_and(|s| s.len() > 128)
        {
            return Err("control field exceeds limit".into());
        }
        if let Some(endpoint) = &self.endpoint {
            validate_endpoint(endpoint)?;
        }
        Ok(())
    }
}
pub fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let host = endpoint
        .strip_prefix("ws://")
        .or_else(|| endpoint.strip_prefix("wss://"))
        .ok_or("use a ws:// or wss:// server address")?;
    if host.is_empty()
        || host.contains('@')
        || host.contains('?')
        || host.contains('#')
        || host.chars().any(char::is_whitespace)
        || host.len() > 512
    {
        return Err("invalid server address; credentials belong in the password field".into());
    }
    Ok(())
}

#[derive(Serialize)]
pub struct UiEvent {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub item_name: String,
    pub sender: String,
    pub flags: u8,
}

pub fn local_item_classifications() -> BTreeMap<i64, u8> {
    // This is the pinned Sly catalogue, never another game's name-based guess.
    let catalogue: Value = serde_json::from_str(include_str!("../../world/mappings.json"))
        .expect("checked-in Sly catalogue must be valid JSON");
    catalogue["items"]
        .as_array()
        .expect("Sly item catalogue")
        .iter()
        .filter_map(|item| {
            Some((
                item["id"].as_i64()?,
                (item["classification"].as_u64()? & 7) as u8,
            ))
        })
        .collect()
}

pub fn display_flags(
    item_id: i64,
    location_id: i64,
    sender_slot: u32,
    server_flags: u8,
    local_catalogue: &BTreeMap<i64, u8>,
) -> (u8, &'static str) {
    // AP's starting inventory packets carry flags=0 even for known progression.
    // Restrict fallback to that exact reserved source; never override an actual
    // location's flags, a nonzero server classification, or another game's item.
    if location_id == -2 && sender_slot == 0 && server_flags == 0 {
        if let Some(flags) = local_catalogue.get(&item_id) {
            return (*flags, "local_precollected_catalog");
        }
        return (0, "unknown_precollected");
    }
    (server_flags, "server")
}

#[derive(Clone)]
pub struct InventoryInput {
    pub index: usize,
    pub item_id: i64,
    pub name: String,
    pub flags: u8,
    pub classification_source: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryRow {
    pub item_id: i64,
    pub name: String,
    pub count: u64,
    pub received_count: u64,
    pub applied_count: u64,
    pub pending_count: u64,
    pub flags: u8,
    pub classification_source: String,
}

/// A pure projection of the complete indexed AP history, not an inventory grant.
/// Applied means acknowledged by the native delivery contract, not still unspent.
pub fn inventory_view(
    history: &[InventoryInput],
    completed: &BTreeSet<usize>,
) -> Result<Vec<InventoryRow>, String> {
    let mut rows = BTreeMap::<i64, InventoryRow>::new();
    for (expected, item) in history.iter().enumerate() {
        if item.index != expected {
            return Err("incomplete or duplicate received-item index".into());
        }
        let row = rows.entry(item.item_id).or_insert_with(|| InventoryRow {
            item_id: item.item_id,
            name: item.name.clone(),
            count: 0,
            received_count: 0,
            applied_count: 0,
            pending_count: 0,
            flags: item.flags,
            classification_source: item.classification_source.clone(),
        });
        row.received_count += 1;
        row.count = row.received_count;
        row.flags |= item.flags;
        if row.classification_source != item.classification_source {
            row.classification_source = "mixed".into();
        }
        if completed.contains(&item.index) {
            row.applied_count += 1;
        } else {
            row.pending_count += 1;
        }
    }
    Ok(rows.into_values().collect())
}

/// Never present a previous session/revision's reachability as current.
pub fn matching_logic(output: Value, revision: u64, identity: &SessionIdentity) -> Option<Value> {
    if output.get("version").and_then(Value::as_u64) != Some(1)
        || output.get("revision").and_then(Value::as_u64) != Some(revision)
        || output.get("identity") != Some(&json!(identity))
    {
        return None;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precollected_episode_uses_known_local_classification_only() {
        let catalog = local_item_classifications();
        assert_eq!(
            display_flags(10020021, -2, 0, 0, &catalog),
            (1, "local_precollected_catalog")
        );
        assert_eq!(display_flags(10020021, 123, 1, 0, &catalog), (0, "server"));
        assert_eq!(display_flags(10020021, -2, 0, 4, &catalog), (4, "server"));
        assert_eq!(
            display_flags(987654, -2, 0, 0, &catalog),
            (0, "unknown_precollected")
        );
    }
    #[test]
    fn inventory_counts_distinct_indexes_without_accumulating_replay() {
        let item = InventoryInput {
            index: 0,
            item_id: 10020015,
            name: "Tide of Terror: Key".into(),
            flags: 1,
            classification_source: "server".into(),
        };
        let mut second = item.clone();
        second.index = 1;
        let history = vec![item, second];
        let completed = BTreeSet::from([0]);
        for _ in 0..2 {
            let rows = inventory_view(&history, &completed).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].received_count, 2);
            assert_eq!(rows[0].applied_count, 1);
            assert_eq!(rows[0].pending_count, 1);
        }
    }
    #[test]
    fn duplicate_or_gapped_history_is_not_an_inventory() {
        let item = InventoryInput {
            index: 0,
            item_id: 10020020,
            name: "1-Up".into(),
            flags: 0,
            classification_source: "server".into(),
        };
        assert!(inventory_view(&[item.clone(), item.clone()], &BTreeSet::new()).is_err());
        let mut gap = item;
        gap.index = 2;
        assert!(inventory_view(&[gap], &BTreeSet::new()).is_err());
    }
    fn sample() -> (Profile, Value) {
        let p: Profile=serde_json::from_value(json!({"bridge_dir":"bridge","journal_path":"journal.json","endpoint":"ws://localhost:38281","slot_name":"Sly","game":GAME,"save":"save","contract":CONTRACT,"items":{},"locations":[]})).unwrap();
        let slot = json!({"Seed":"test","UnlockClockwerk":1,"RequiredBosses":1,"MaxPages":25,"RequiredPages":20,"StartingEpisode":1,"FastClockwerk":1,"IncludeHourglasses":1,"HourglassesRequireRoll":0,"AvoidEarlyBK":1,"CutsceneSkip":1,"LocationCluesanityBundleSize":1,"ItemCluesanityBundleSize":0,"MinigameCaches":0,"TrapChance":0,"IcePhysicsTrapWeight":25,"SpeedChangeTrapWeight":25,"InvisibilityTrapWeight":25,"BallTrapWeight":25,"EnableTricks":[],"ExcludeMinigames":["Crabs","Races","Turrets","Hover Blasters","Chicken Killing","Swamp Skiff"]});
        (p, slot)
    }
    #[test]
    fn old_world_flat_slot_is_accepted() {
        let (p, s) = sample();
        assert!(validate_slot(&p, &s).is_ok());
    }
    #[test]
    fn malformed_or_future_slot_rejected() {
        let (p, mut s) = sample();
        s["StartingEpisode"] = json!(5);
        assert!(validate_slot(&p, &s).is_err());
        s["StartingEpisode"] = json!(1);
        s["EnableTricks"] = json!(["invented"]);
        assert!(validate_slot(&p, &s).is_err());
        s["EnableTricks"] = json!([]);
        s["bridge_version"] = json!(2);
        assert!(validate_slot(&p, &s).is_err());
    }
    #[test]
    fn no_alias_or_wrong_game_contract() {
        let (mut p, s) = sample();
        p.game = "Sly 1".into();
        assert!(validate_slot(&p, &s).is_err());
    }
    #[test]
    fn rejects_foreign_control() {
        let c: Control =
            serde_json::from_value(json!({"version":1,"id":"a","save":"old","action":"connect"}))
                .unwrap();
        assert!(c.validate("new").is_err());
    }
    #[test]
    fn no_chat_control() {
        let c: Control =
            serde_json::from_value(json!({"version":1,"id":"a","save":"run","action":"say"}))
                .unwrap();
        assert!(c.validate("run").is_err());
    }
    #[test]
    fn endpoint_cannot_contain_secrets() {
        assert!(validate_endpoint("ws://localhost:38281").is_ok());
        assert!(validate_endpoint("ws://secret@host").is_err());
        assert!(validate_endpoint("ws://host?password=secret").is_err());
    }
    #[test]
    fn stale_logic_is_not_a_bk_result() {
        let identity = SessionIdentity {
            game: GAME.into(),
            seed: "test".into(),
            team: 0,
            slot: 1,
            save: "one".into(),
        };
        let output = json!({"version":1,"revision":2,"identity":identity,"bk":true});
        assert!(matching_logic(output.clone(), 3, &identity).is_none());
        assert!(matching_logic(output, 2, &identity).is_some());
    }
}
