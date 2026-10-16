use client_ui::{ActivityEvent, ActivityKind, ClientSnapshot, DeliveryState};

/// A snapshot from before optional status/feed metadata was added. This is a literal previous
/// wire shape rather than serialization of today's struct, so a missing serde default fails here.
const LEGACY_SNAPSHOT: &str = r#"{
    "revision": 8,
    "process": "Attached",
    "ap": "Authenticated",
    "delivery": "Ready",
    "server": "localhost:38281",
    "slot": "Hunter",
    "seed": "previous-seed",
    "goal": "Victory",
    "go_mode": false,
    "locations": {"checked": 4, "total": 20},
    "ledger": {"queued": 3, "delivered": 5, "storage_routed": null, "parked": 0},
    "activity": [{"sequence": 1, "kind": "ReceivedItem", "text": "Saw Cleaver"}],
    "stale": false
}"#;

#[test]
fn previous_snapshot_loads_through_the_original_public_path() {
    let snapshot: ClientSnapshot = json::from_str(LEGACY_SNAPSHOT).unwrap();
    // A type assignment also proves these are actual re-exports, so a renderer or adapter can
    // exchange the snapshot without converting between subtly different copies of the schema.
    let universal: peliarch_client_core::ui::ClientSnapshot = snapshot.clone();
    assert_eq!(universal, snapshot);
    assert_eq!(snapshot.delivery, DeliveryState::Ready);
    assert_eq!(snapshot.seed.as_deref(), Some("previous-seed"));
    assert_eq!(snapshot.ledger.delivered, 5);
    assert_eq!(snapshot.activity[0].timestamp_ms, 0);
    assert_eq!(snapshot.activity[0].item_class, None);
    assert_eq!(snapshot.version, None);
    assert_eq!(snapshot.victory, None);
    assert_eq!(snapshot.tracker, None);
    assert_eq!(snapshot.game_pid, None);
    assert!(!snapshot.tracker_only);
    assert!(!snapshot.observer_only);
    assert!(snapshot.blocked.is_empty());
    assert_eq!(snapshot.receive_cursor, None);
    assert!(!snapshot.gameplay_ready);
    assert_eq!(
        json::from_str::<ClientSnapshot>(&json::to_string(&snapshot).unwrap()).unwrap(),
        snapshot
    );
}

#[test]
fn observer_mode_round_trips_without_changing_legacy_default_output() {
    let mut value = json::to_value(ClientSnapshot::default()).unwrap();
    assert!(value.get("observer_only").is_none());
    value["observer_only"] = json::json!(true);
    let snapshot: ClientSnapshot = json::from_value(value).unwrap();
    assert!(snapshot.observer_only);
    assert_eq!(json::to_value(snapshot).unwrap()["observer_only"], true);

    let mut explicit_false = json::to_value(ClientSnapshot::default()).unwrap();
    explicit_false["observer_only"] = json::json!(false);
    let snapshot: ClientSnapshot = json::from_value(explicit_false).unwrap();
    assert!(!snapshot.observer_only);
    assert!(
        json::to_value(snapshot)
            .unwrap()
            .get("observer_only")
            .is_none()
    );
}

#[test]
fn current_snapshot_keeps_every_serialized_field_and_enum_spelling() {
    // Pin the old client-ui writer's complete current default shape. Merely comparing the
    // compatibility facade to core would miss an accidental schema edit made to both paths.
    let expected = json::json!({
        "revision": 0,
        "process": "Waiting",
        "ap": "Disconnected",
        "delivery": "NotArmed",
        "delivery_detail": null,
        "server": null,
        "slot": null,
        "seed": null,
        "version": null,
        "goal": null,
        "go_mode": null,
        "victory": null,
        "locations": null,
        "unchecked_locations": [],
        "tracker": null,
        "game_pid": null,
        "tracker_only": false,
        "ledger": {"queued": 0, "delivered": 0, "storage_routed": null, "parked": 0},
        "blocked": [],
        "save_identity": null,
        "gameplay_ready": false,
        "receive_cursor": null,
        "activity": [],
        "stale": false
    });
    assert_eq!(json::to_value(ClientSnapshot::default()).unwrap(), expected);
    let previous_feed = r#"{"sequence":7,"kind":"ReceivedItem","text":"Golden Seed"}"#;
    let event: ActivityEvent = json::from_str(previous_feed).unwrap();
    assert_eq!(event.kind, ActivityKind::ReceivedItem);
    assert_eq!(
        json::to_value(event).unwrap(),
        json::json!({
            "sequence": 7,
            "kind": "ReceivedItem",
            "text": "Golden Seed",
            "timestamp_ms": 0,
            "item_class": null
        })
    );
}

#[test]
fn tracker_snapshot_keeps_the_existing_tracker_wire_shape() {
    let mut value = json::to_value(ClientSnapshot::default()).unwrap();
    let tracker = json::json!({
        "regions": [{
            "name": "Central Yharnam", "checked": 2, "total": 4,
            "in_logic": ["Cleric Beast"], "quest": ["Viola's Daughter"],
            "needs": null
        }],
        "hints": ["Hunter Chief Emblem - Central Yharnam: Cleric Beast"],
        "hints_used": 1,
        "hint_offer": "Available"
    });
    value["tracker"] = tracker.clone();
    value["tracker_only"] = json::json!(true);
    value["game_pid"] = json::json!(1234);
    let snapshot: ClientSnapshot = json::from_value(value).unwrap();
    let portable: peliarch_client_core::ui::ClientSnapshot = snapshot;
    assert_eq!(portable.tracker.as_ref().unwrap().in_logic_count(), 1);
    let serialized = json::to_value(portable).unwrap();
    assert_eq!(serialized["tracker"], tracker);
    assert_eq!(serialized["tracker_only"], true);
    assert_eq!(serialized["game_pid"], 1234);
}
