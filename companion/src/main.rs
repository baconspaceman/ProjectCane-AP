use archipelago_rs::{Client, Event, Print, ReceivedItem};
use peliarch_client_core::{
    AdapterSnapshot, Capabilities, DeliveryState, Effect, EffectKind, EffectOutcome, GameAdapter,
    Observation, Readiness, SessionIdentity,
    connection::ConnectionProfile,
    runner::{GameIntegration, ProtocolSession, Runner, RunnerConfig},
    store::JournalPath,
};
use serde_json::{Value, json};
use sly_ap_companion::{
    Control, InventoryInput, UiEvent, display_flags, inventory_view, local_item_classifications,
    matching_logic, validate_endpoint, validate_slot,
};
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use universal_ap_bridge::{FileAdapter, Handshake, Profile, Snapshot, read_json, write_json};

struct SlyAdapter {
    inner: FileAdapter,
    catalogue: BTreeSet<i64>,
}
impl GameAdapter for SlyAdapter {
    type Payload = Value;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn snapshot(&mut self) -> AdapterSnapshot {
        self.inner.snapshot()
    }
    fn observe_effect(&mut self, e: &Effect<Value>) -> Observation<bool> {
        self.inner.observe_effect(e)
    }
    fn apply(&mut self, i: &SessionIdentity, e: &Effect<Value>) -> EffectOutcome {
        self.inner.apply(i, e)
    }
}
impl GameIntegration for SlyAdapter {
    type SlotData = Value;
    fn validate_slot(&self, data: &Value) -> Result<(), String> {
        self.inner.validate_slot(data)
    }
    fn pair_session(
        &mut self,
        s: &ProtocolSession,
        data: &Value,
    ) -> Result<SessionIdentity, String> {
        if data.get("Seed").and_then(Value::as_str) != Some(s.seed.as_str()) {
            return Err("Sly slot Seed does not match authenticated room".into());
        }
        self.inner.pair_session(s, data)
    }
    fn map_item(&self, item: &ReceivedItem) -> Result<Option<(EffectKind, Value)>, String> {
        self.inner.map_item(item)
    }
    fn protocol_tick(&mut self, client: &mut Client<Value>, _: &[Event]) -> Result<(), String> {
        let active: BTreeSet<i64> = client
            .checked_locations()
            .chain(client.unchecked_locations())
            .map(|l| l.id())
            .collect();
        if !active.is_subset(&self.catalogue) {
            return Err("seed has checks outside this native contract".into());
        }
        let handshake: Handshake = read_json(&self.inner.profile.bridge_dir.join("session.json"))?;
        // Native intersects physical observations with this seed-specific set.
        // Publish before Runner polls the game snapshot so a stale set cannot
        // accidentally turn disabled check families into server checks.
        let path = self.inner.profile.bridge_dir.join("active-locations.json");
        let frame = json!({"version":1,"identity":handshake.identity,"challenge":handshake.challenge,"locations":active});
        if read_json::<Value>(&path).ok().as_ref() != Some(&frame) {
            write_json(&path, &frame)?;
        }
        self.inner.profile.locations = active;
        Ok(())
    }
}
fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn game_facts(dir: &Path, identity: &SessionIdentity, ready: bool) -> Value {
    if !ready {
        return json!({});
    }
    let result = (|| -> Result<Value, String> {
        let h: Handshake = read_json(&dir.join("session.json"))?;
        let s: Snapshot = read_json(&dir.join("snapshot.json"))?;
        let facts: Value = read_json(&dir.join("game-facts.json"))?;
        if h.identity != *identity
            || s.identity != *identity
            || s.challenge != h.challenge
            || facts["identity"] != json!(identity)
            || facts["challenge"] != json!(h.challenge)
            || facts["boot"] != json!(s.boot)
            || facts["version"] != json!(1)
        {
            return Err("foreign facts".into());
        }
        Ok(
            json!({"observed_events":facts["observed_events"],"events_complete":facts["events_complete"]}),
        )
    })();
    result.unwrap_or_else(|_| json!({}))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!(
            "sly-ap-companion PROFILE.json [--connect] [--ticks N]\nStarts disconnected unless --connect. AP_PASSWORD supplies an optional runtime password. Native UI controls connect/reconnect/disconnect. No chat or native rewards are implemented here."
        );
        return Ok(());
    }
    let profile_path = args.get(1).ok_or("profile path required; use --help")?;
    let p = Profile::load(Path::new(profile_path))?;
    validate_endpoint(&p.endpoint)?;
    if p.game != sly_ap_companion::GAME || p.contract != sly_ap_companion::CONTRACT {
        return Err("not a Sly native v1 profile".into());
    }
    let mut adapter = SlyAdapter {
        catalogue: p.locations.clone(),
        inner: FileAdapter::open(p.clone())?
            .with_slot_validator(validate_slot)
            .with_slot_data(),
    };
    let mut store = JournalPath::<Value>::new(&p.journal_path);
    let mut runner = Runner::new(
        RunnerConfig {
            endpoint: p.endpoint.clone(),
            slot_name: p.slot_name.clone(),
            game: p.game.clone(),
            profile: ConnectionProfile::new(true, std::env::var("AP_PASSWORD").ok()),
            reconnect_delay: Duration::from_secs(2),
            report_interval: Duration::from_millis(250),
        },
        Instant::now(),
    );
    let mut enabled = args.iter().any(|a| a == "--connect");
    if !enabled {
        runner.disconnect();
    }
    let limit = args
        .iter()
        .position(|a| a == "--ticks")
        .map(|i| {
            args.get(i + 1)
                .ok_or("missing tick count")?
                .parse::<u64>()
                .map_err(|_| "invalid tick count")
        })
        .transpose()?;
    let controls = p.bridge_dir.join("controls");
    fs::create_dir_all(&controls).map_err(|e| e.to_string())?;
    let mut seen_controls = BTreeSet::new();
    let mut control_results = VecDeque::new();
    let mut events = VecDeque::<UiEvent>::new();
    let mut seen_items = BTreeSet::new();
    let local_classifications = local_item_classifications();
    let mut history_ready = false;
    // A restarted host must not accept an old worker result just because both
    // processes happened to use revision 1 for the same profile.
    let mut revision = u64::try_from(unix_ms()).unwrap_or(u64::MAX / 2);
    let mut previous_input = Value::Null;
    let mut ticks = 0;
    loop {
        let mut paths: Vec<_> = fs::read_dir(&controls)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .take(64)
            .collect();
        paths.sort();
        for path in paths {
            // Unlink even invalid requests without echoing their contents (may contain credentials).
            let request = read_json::<Control>(&path);
            fs::remove_file(&path).map_err(|_| "cannot consume local control request")?;
            let Ok(mut request) = request else { continue };
            let id = request.id.clone();
            let result = request.validate(&p.save).and_then(|_| {
                if !seen_controls.insert(id.clone()) {
                    return Err("duplicate control request".into());
                }
                if seen_controls.len() > 10_000 {
                    return Err("control limit reached; restart companion".into());
                }
                match request.action.as_str() {
                    "disconnect" => {
                        runner.disconnect();
                        enabled = false;
                        history_ready = false;
                    }
                    "reconnect" => {
                        runner.reconnect(Instant::now());
                        enabled = true;
                        history_ready = false;
                    }
                    "connect" => {
                        let endpoint = request
                            .endpoint
                            .take()
                            .unwrap_or_else(|| runner.config().endpoint.clone());
                        let slot = request
                            .slot_name
                            .take()
                            .unwrap_or_else(|| runner.config().slot_name.clone());
                        if slot.is_empty() {
                            return Err("slot name is empty".into());
                        }
                        runner.connect(endpoint, slot, request.password.take(), Instant::now());
                        enabled = true;
                        history_ready = false;
                    }
                    _ => unreachable!(),
                }
                Ok(())
            });
            control_results.push_back(json!({"id":id,"ok":result.is_ok(),"error":result.err()}));
            while control_results.len() > 32 {
                control_results.pop_front();
            }
        }
        let report = runner.poll(Instant::now(), &mut adapter, &mut store);
        let (connected, ready, diagnostic) = match &report {
            Ok(r) => {
                if r.events
                    .iter()
                    .any(|e| matches!(e, Event::ReceivedItems(0)))
                {
                    history_ready = true;
                }
                if !r.connected {
                    history_ready = false;
                }
                (
                    r.connected,
                    r.step
                        .as_ref()
                        .is_some_and(|s| s.snapshot.readiness == Readiness::Ready),
                    r.diagnostic.clone(),
                )
            }
            Err(_) => {
                history_ready = false;
                (false,false,Some("Connection or game contract failed; check matching versions, slot and save profile.".into()))
            }
        };
        let identity = runner.host().map(|h| h.identity().clone());
        let mut input = json!({"version":1,"identity":identity,"connected":false,"history_ready":false,"slot_data":{},"received_items":[],"checked_locations":[],"missing_locations":[],"game_facts":{}});
        let mut players = Vec::new();
        let mut inventory = Vec::new();
        let mut inventory_status = "unknown";
        if connected && let (Some(client), Some(identity)) = (runner.client(), identity.as_ref()) {
            let mut inventory_inputs = Vec::new();
            let received:Vec<Value>=client.received_items().iter().map(|item|{
                let server_flags=(u8::from(item.is_progression())) | (u8::from(item.is_useful())<<1) | (u8::from(item.is_trap())<<2);
                let (flags,classification_source)=display_flags(item.item().id(),item.location().id(),item.sender().slot(),server_flags,&local_classifications);
                let id=format!("{}:{}:{}:{}",identity.seed,identity.team,identity.slot,item.index());
                if history_ready && seen_items.insert(id.clone()) {
                    events.push_back(UiEvent{id,kind:"received".into(),text:format!("{} sent you {}",item.sender().alias(),item.item().name()),item_name:item.item().name().to_string(),sender:item.sender().alias().into(),flags});
                }
                inventory_inputs.push(InventoryInput{index:item.index(),item_id:item.item().id(),name:item.item().name().to_string(),flags,classification_source:classification_source.into()});
                json!({"index":item.index(),"item_id":item.item().id(),"item_name":item.item().name().to_string(),"location_id":item.location().id(),"player":item.sender().slot(),"flags":flags,"server_flags":server_flags,"classification_source":classification_source})
            }).collect();
            if history_ready {
                let completed: BTreeSet<usize> = runner
                    .host()
                    .map(|host| {
                        host.records()
                            .filter(|record| matches!(&record.state, DeliveryState::Completed))
                            .filter_map(|record| {
                                record.effect.id.0.strip_prefix("received:")?.parse().ok()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let Ok(rows) = inventory_view(&inventory_inputs, &completed) {
                    inventory = rows;
                    inventory_status = "known";
                }
            }
            let checked: BTreeSet<_> = client.server_checked_locations().map(|l| l.id()).collect();
            let all: BTreeSet<_> = client
                .checked_locations()
                .chain(client.unchecked_locations())
                .map(|l| l.id())
                .collect();
            let missing: Vec<_> = all.difference(&checked).copied().collect();
            input = json!({"version":1,"identity":identity,"connected":true,"history_ready":history_ready,"slot_data":client.slot_data(),"received_items":received,"checked_locations":checked,"missing_locations":missing,"game_facts":game_facts(&p.bridge_dir,identity,ready)});
            players=client.players().map(|player|json!({"team":player.team(),"slot":player.slot(),"name":player.alias(),"game":player.game().to_string()})).collect();
            if let Ok(report) = &report {
                for event in &report.events {
                    if let Event::Print(Print::ItemSend { item, .. }) = event
                        && item.sender().slot() == identity.slot
                        && item.sender().team() == identity.team
                        && item.receiver().slot() != identity.slot
                    {
                        let id = format!(
                            "sent:{}:{}:{}:{}",
                            identity.seed,
                            identity.team,
                            identity.slot,
                            item.location().id()
                        );
                        if seen_items.insert(id.clone()) {
                            let flags = u8::from(item.is_progression())
                                | (u8::from(item.is_useful()) << 1)
                                | (u8::from(item.is_trap()) << 2);
                            events.push_back(UiEvent {
                                id,
                                kind: "sent".into(),
                                text: format!(
                                    "Sent {} to {}",
                                    item.item().name(),
                                    item.receiver().alias()
                                ),
                                item_name: item.item().name().to_string(),
                                sender: item.receiver().alias().into(),
                                flags,
                            });
                        }
                    }
                }
            }
        }
        while events.len() > 100 {
            events.pop_front();
        }
        if input != previous_input {
            revision += 1;
            previous_input = input.clone();
            input["revision"] = json!(revision);
            write_json(&p.bridge_dir.join("logic-input.json"), &input)?;
        }
        let tracker = if connected && history_ready {
            identity.as_ref().and_then(|identity| {
                read_json(&p.bridge_dir.join("logic-output.json"))
                    .ok()
                    .and_then(|v| matching_logic(v, revision, identity))
            })
        } else {
            None
        };
        let deliveries: Vec<_> = runner
            .host()
            .map(|h| {
                h.records()
                    .map(|r| json!({"id":r.effect.id,"state":r.state}))
                    .collect()
            })
            .unwrap_or_default();
        write_json(
            &p.bridge_dir.join("ui-status.json"),
            &json!({"version":1,"revision":revision,"updated_unix_ms":unix_ms(),"identity":identity,"save":p.save,"endpoint":runner.config().endpoint,"slot_name":runner.config().slot_name,"server_connected":connected,"game_ready":ready,"connection_enabled":enabled,"diagnostic":diagnostic,"tracker":tracker,"players":players,"inventory":inventory,"inventory_status":inventory_status,"deliveries":deliveries,"capabilities":{"links":false,"ghosts":false},"control_results":control_results}),
        )?;
        write_json(
            &p.bridge_dir.join("ui-events.json"),
            &json!({"version":1,"identity":identity,"events":events}),
        )?;
        ticks += 1;
        if limit.is_some_and(|limit| ticks >= limit) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    runner.disconnect();
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Sly companion stopped: {error}");
        std::process::exit(1);
    }
}
