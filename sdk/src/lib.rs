//! Versioned local bridge. The host owns delivery orchestration; the game owns
//! durable effect receipts. This transport is local trusted IPC, not an auth boundary.
pub use peliarch_client_core::{Effect, EffectId, EffectKind, EffectOutcome, SessionIdentity};
use peliarch_client_core::{
    runner::{GameIntegration, ProtocolSession},
    store::write_file_atomic,
    *,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1_048_576;
pub const MAX_RECEIPTS: usize = 10_000;
pub type Result<T> = std::result::Result<T, String>;
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub fn nonce() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}

/// Read limits apply before deserialization, including files that grow mid-read.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take((MAX_FRAME + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("frame exceeds 1 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("frame exceeds 1 MiB".into());
    }
    // Windows scanners/readers or resource pressure can briefly prevent replace.
    // Re-publish exactly the same bytes under the role's writer lock; never rerun
    // a game effect. Persistent resource exhaustion still fails after 10 waits.
    for attempt in 0..=10 {
        match write_file_atomic(path, &bytes) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == 10 || !is_transient_replace(&error.to_string()) => {
                return Err(format!("{}: {error}", path.display()));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    unreachable!()
}
fn is_transient_replace(error: &str) -> bool {
    cfg!(windows)
        && [
            "(os error 5)",
            "(os error 32)",
            "(os error 33)",
            "(os error 1450)",
        ]
        .iter()
        .any(|code| error.contains(code))
}
fn lock(dir: &Path, role: &str) -> Result<File> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(format!("{role}.lock")))
        .map_err(|e| e.to_string())?;
    f.try_lock()
        .map_err(|e| format!("another {role} owns this bridge: {e}"))?;
    Ok(f)
}
fn check_version(v: u32) -> Result<()> {
    if v != VERSION {
        Err(format!("unsupported bridge version {v}"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemMapping {
    pub kind: EffectKind,
    pub payload: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub bridge_dir: PathBuf,
    pub journal_path: PathBuf,
    pub endpoint: String,
    pub slot_name: String,
    pub game: String,
    pub save: String,
    pub contract: String,
    pub items: BTreeMap<i64, ItemMapping>,
    pub locations: BTreeSet<i64>,
    #[serde(default = "default_timeout")]
    pub ack_timeout_ms: u64,
    #[serde(default = "default_stale")]
    pub stale_after_ms: u64,
}
fn default_timeout() -> u64 {
    2000
}
fn default_stale() -> u64 {
    1500
}
impl Profile {
    pub fn load(path: &Path) -> Result<Self> {
        let mut p: Self = read_json(path)?;
        let base = path.parent().unwrap_or(Path::new("."));
        if p.bridge_dir.is_relative() {
            p.bridge_dir = base.join(&p.bridge_dir);
        }
        if p.journal_path.is_relative() {
            p.journal_path = base.join(&p.journal_path);
        }
        p.validate()?;
        Ok(p)
    }
    pub fn validate(&self) -> Result<()> {
        if self.game.is_empty()
            || self.save.is_empty()
            || self.contract.is_empty()
            || self.slot_name.is_empty()
        {
            return Err("game, save, contract and slot_name must be nonempty".into());
        }
        if !(50..=30000).contains(&self.ack_timeout_ms)
            || !(100..=30000).contains(&self.stale_after_ms)
        {
            return Err("invalid timeout bounds".into());
        }
        if self.items.keys().any(|id| *id <= 0) || self.locations.iter().any(|id| *id <= 0) {
            return Err("item and location IDs must be positive".into());
        }
        if self.items.len() > MAX_RECEIPTS || self.locations.len() > MAX_RECEIPTS {
            return Err("profile exceeds catalogue bound".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handshake {
    pub version: u32,
    pub contract: String,
    pub challenge: String,
    pub identity: SessionIdentity,
    /// Validated AP options, published only by explicitly opted-in clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_data: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub contract: String,
    pub challenge: String,
    pub identity: SessionIdentity,
    pub boot: String,
    pub heartbeat: u64,
    pub capabilities: Capabilities,
    pub ready: bool,
    pub checked_locations: BTreeSet<i64>,
    pub goal: bool,
    pub receipts: BTreeMap<String, Effect<Value>>,
    pub permanent_states: BTreeMap<String, bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub version: u32,
    pub challenge: String,
    pub boot: String,
    pub attempt: String,
    pub identity: SessionIdentity,
    pub effect: Effect<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    pub version: u32,
    pub challenge: String,
    pub boot: String,
    pub attempt: String,
    pub identity: SessionIdentity,
    pub effect: Effect<Value>,
    pub outcome: EffectOutcome,
}
#[derive(Default)]
struct Liveness {
    anchor: Option<(String, u64, Instant, bool)>,
}
impl Liveness {
    fn observe(&mut self, boot: &str, heartbeat: u64, now: Instant, stale: Duration) -> bool {
        if let Some((old, n, at, trusted)) = &mut self.anchor
            && old == boot
            && heartbeat >= *n
        {
            if heartbeat > *n {
                *n = heartbeat;
                *at = now;
                *trusted = true;
            }
            return *trusted && now.duration_since(*at) <= stale;
        }
        self.anchor = Some((boot.into(), heartbeat, now, false));
        false
    }
}

pub struct FileAdapter {
    pub profile: Profile,
    _lock: File,
    handshake: Option<Handshake>,
    live: Liveness,
    slot_validator: fn(&Profile, &Value) -> Result<()>,
    publish_slot_data: bool,
    capabilities: Capabilities,
}
impl FileAdapter {
    pub fn open(profile: Profile) -> Result<Self> {
        profile.validate()?;
        let held = lock(&profile.bridge_dir, "client")?;
        Ok(Self {
            profile,
            _lock: held,
            handshake: None,
            live: Liveness::default(),
            slot_validator: validate_bridge_slot,
            publish_slot_data: false,
            capabilities: bridge_capabilities(),
        })
    }
    /// Replace APWorld validation for a reviewed existing-world contract.
    /// This does not bypass profile, local protocol or session identity checks.
    pub fn with_slot_validator(mut self, validator: fn(&Profile, &Value) -> Result<()>) -> Self {
        self.slot_validator = validator;
        self
    }
    /// Publish validated AP options. Both endpoints must support this extension;
    /// original v1 readers reject unknown fields. Off by default.
    pub fn with_slot_data(mut self) -> Self {
        self.publish_slot_data = true;
        self
    }
    /// Select the services implemented by a partial game adapter. The peer must
    /// advertise exactly this set. DeathLink is not implemented by this transport.
    pub fn with_capabilities(mut self, capabilities: Capabilities) -> Result<Self> {
        if capabilities.death_link
            || self
                .profile
                .items
                .values()
                .any(|mapping| match mapping.kind {
                    EffectKind::Permanent => !capabilities.permanent_effects,
                    EffectKind::Repeatable => !capabilities.repeatable_effects,
                })
        {
            return Err("unsupported capability or profile effect mapping".into());
        }
        self.capabilities = capabilities;
        Ok(self)
    }
    pub fn bind(&mut self, identity: SessionIdentity) -> Result<()> {
        self.bind_options(identity, None)
    }
    fn bind_options(&mut self, identity: SessionIdentity, slot_data: Option<Value>) -> Result<()> {
        if identity.game != self.profile.game
            || identity.save != self.profile.save
            || identity.seed.is_empty()
        {
            return Err("session does not match configured game/save".into());
        }
        let h = Handshake {
            version: VERSION,
            contract: self.profile.contract.clone(),
            challenge: nonce(),
            identity,
            slot_data,
        };
        write_json(&self.profile.bridge_dir.join("session.json"), &h)?;
        self.handshake = Some(h);
        self.live = Liveness::default();
        Ok(())
    }
    fn current(&mut self) -> Result<Snapshot> {
        let h = self.handshake.as_ref().ok_or("not paired")?;
        let s: Snapshot = read_json(&self.profile.bridge_dir.join("snapshot.json"))?;
        check_version(s.version)?;
        if s.contract != h.contract || s.challenge != h.challenge || s.identity != h.identity {
            return Err("snapshot session/challenge/contract mismatch".into());
        }
        if s.boot.is_empty() || s.heartbeat == 0 {
            return Err("missing boot/heartbeat".into());
        }
        if s.capabilities != self.capabilities
            || (!self.capabilities.locations && !s.checked_locations.is_empty())
            || (!self.capabilities.goal && s.goal)
        {
            return Err("unsupported capability set".into());
        }
        if !s.checked_locations.is_subset(&self.profile.locations)
            || s.receipts.len() > MAX_RECEIPTS
        {
            return Err("snapshot catalogue/receipt bounds invalid".into());
        }
        if !self.live.observe(
            &s.boot,
            s.heartbeat,
            Instant::now(),
            Duration::from_millis(self.profile.stale_after_ms),
        ) {
            return Err("waiting for fresh advancing game heartbeat".into());
        }
        Ok(s)
    }
}
impl GameAdapter for FileAdapter {
    type Payload = Value;
    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }
    fn snapshot(&mut self) -> AdapterSnapshot {
        match self.current() {
            Ok(s) => AdapterSnapshot {
                identity: Observation::Known(s.identity),
                readiness: if s.ready {
                    Readiness::Ready
                } else {
                    Readiness::NotReady("game is not ready".into())
                },
                checked_locations: if self.capabilities.locations {
                    Observation::Known(s.checked_locations)
                } else {
                    Observation::Unknown("location reporting unsupported".into())
                },
                goal: if self.capabilities.goal {
                    Observation::Known(s.goal)
                } else {
                    Observation::Unknown("goal reporting unsupported".into())
                },
            },
            Err(e) => AdapterSnapshot {
                identity: Observation::Unknown(e.clone()),
                readiness: Readiness::NotReady(e.clone()),
                checked_locations: Observation::Unknown(e.clone()),
                goal: Observation::Unknown(e),
            },
        }
    }
    fn observe_effect(&mut self, e: &Effect<Value>) -> Observation<bool> {
        match self.current() {
            Ok(s) => match s.receipts.get(&e.id.0) {
                Some(record) if record == e => match s.permanent_states.get(&e.id.0) {
                    Some(value) => Observation::Known(*value),
                    None => Observation::Unknown("no permanent-state observation".into()),
                },
                Some(_) => Observation::Unknown("receipt payload mismatch".into()),
                None => Observation::Known(false),
            },
            Err(reason) => Observation::Unknown(reason),
        }
    }
    fn apply(&mut self, expected: &SessionIdentity, e: &Effect<Value>) -> EffectOutcome {
        let s = match self.current() {
            Ok(s) => s,
            Err(reason) => return EffectOutcome::NotReady(reason),
        };
        if &s.identity != expected {
            return EffectOutcome::Rejected("session mismatch".into());
        }
        if !s.ready {
            return EffectOutcome::NotReady("game is not ready; nothing queued".into());
        }
        let c = Command {
            version: VERSION,
            challenge: s.challenge,
            boot: s.boot,
            attempt: nonce(),
            identity: expected.clone(),
            effect: e.clone(),
        };
        if let Err(reason) = write_json(&self.profile.bridge_dir.join("command.json"), &c) {
            return EffectOutcome::Uncertain(format!("command publication failed: {reason}"));
        }
        let deadline = Instant::now() + Duration::from_millis(self.profile.ack_timeout_ms);
        while Instant::now() < deadline {
            if let Ok(a) = read_json::<Ack>(&self.profile.bridge_dir.join("ack.json"))
                && a.version == VERSION
                && a.challenge == c.challenge
                && a.boot == c.boot
                && a.attempt == c.attempt
                && a.identity == c.identity
                && a.effect == c.effect
            {
                return a.outcome;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        EffectOutcome::Uncertain(
            "command queued; durable acknowledgement timed out; explicit recovery required".into(),
        )
    }
}
pub fn bridge_capabilities() -> Capabilities {
    Capabilities {
        locations: true,
        goal: true,
        death_link: false,
        permanent_effects: true,
        repeatable_effects: true,
    }
}
impl GameIntegration for FileAdapter {
    type SlotData = Value;
    fn validate_slot(&self, slot: &Value) -> Result<()> {
        (self.slot_validator)(&self.profile, slot)
    }
    fn pair_session(&mut self, s: &ProtocolSession, slot: &Value) -> Result<SessionIdentity> {
        self.validate_slot(slot)?;
        let identity = SessionIdentity {
            game: s.game.clone(),
            seed: s.seed.clone(),
            team: s.team,
            slot: s.slot,
            save: self.profile.save.clone(),
        };
        self.bind_options(
            identity.clone(),
            self.publish_slot_data.then(|| slot.clone()),
        )?;
        Ok(identity)
    }
    fn map_item(&self, item: &archipelago_rs::ReceivedItem) -> Result<Option<(EffectKind, Value)>> {
        self.profile
            .items
            .get(&item.item().id())
            .map(|m| Some((m.kind, m.payload.clone())))
            .ok_or_else(|| {
                format!(
                    "unknown AP item {}; update agreed contract",
                    item.item().id()
                )
            })
    }
}

fn validate_bridge_slot(profile: &Profile, slot: &Value) -> Result<()> {
    if slot.get("bridge_version").and_then(Value::as_u64) != Some(VERSION as u64)
        || slot.get("bridge_contract").and_then(Value::as_str) != Some(profile.contract.as_str())
    {
        return Err("APWorld slot_data must match bridge_version and bridge_contract".into());
    }
    Ok(())
}

/// The durable document is the reference game's authoritative save. Receipt and
/// changed state are committed together. Never mutate an external game in the
/// transaction closure: its effects would not share this atomic commit.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameSave {
    pub version: u32,
    pub contract: String,
    pub identity: SessionIdentity,
    pub state: Value,
    pub receipts: BTreeMap<String, Effect<Value>>,
    pub checked_locations: BTreeSet<i64>,
    pub goal: bool,
}
pub struct GameEndpoint {
    profile: Profile,
    _lock: File,
    boot: String,
    heartbeat: u64,
    save: Option<GameSave>,
    initial: Value,
    last_attempt: Option<String>,
    pending_ack: Option<Ack>,
    slot_data: Option<Value>,
}
impl GameEndpoint {
    pub fn open(profile: Profile, initial: Value) -> Result<Self> {
        profile.validate()?;
        let held = lock(&profile.bridge_dir, "game")?;
        let path = profile.bridge_dir.join("game-save.json");
        let save = if path.exists() {
            let s: GameSave = read_json(&path)?;
            check_version(s.version)?;
            if s.contract != profile.contract
                || s.identity.game != profile.game
                || s.identity.save != profile.save
            {
                return Err("game save/profile mismatch".into());
            }
            if s.identity.seed.is_empty()
                || s.receipts.len() > MAX_RECEIPTS
                || !s.checked_locations.is_subset(&profile.locations)
                || s.receipts.iter().any(|(id, e)| {
                    id != &e.id.0
                        || !profile
                            .items
                            .values()
                            .any(|m| m.kind == e.kind && m.payload == e.payload)
                })
            {
                return Err("game save contains invalid identity, checks or receipts".into());
            }
            Some(s)
        } else {
            None
        };
        Ok(Self {
            profile,
            _lock: held,
            boot: nonce(),
            heartbeat: 0,
            save,
            initial,
            last_attempt: None,
            pending_ack: None,
            slot_data: None,
        })
    }
    pub fn save(&self) -> Option<&GameSave> {
        self.save.as_ref()
    }
    /// Options from the latest successfully validated handshake tick. Not saved
    /// or interpreted here; cleared when the next handshake is absent/invalid.
    pub fn slot_data(&self) -> Option<&Value> {
        self.slot_data.as_ref()
    }
    fn commit(&mut self, s: GameSave) -> Result<()> {
        if s.receipts.len() > MAX_RECEIPTS
            || !s.checked_locations.is_subset(&self.profile.locations)
        {
            return Err("game save bounds/catalogue invalid".into());
        }
        write_json(&self.profile.bridge_dir.join("game-save.json"), &s)?;
        self.save = Some(s);
        Ok(())
    }
    /// Called at a safe game tick. Closure must be pure: compute a replacement
    /// state, not perform external I/O, grant live inventory or change saves.
    pub fn tick<F>(&mut self, ready: bool, transaction: F) -> Result<()>
    where
        F: FnMut(&Value, &Effect<Value>) -> Result<Value>,
    {
        self.tick_with_observer(ready, transaction, |_, _| {
            Err("permanent effects require an explicit state observer".into())
        })
    }
    pub fn tick_with_observer<F, O>(
        &mut self,
        ready: bool,
        mut transaction: F,
        mut observe: O,
    ) -> Result<()>
    where
        F: FnMut(&Value, &Effect<Value>) -> Result<Value>,
        O: FnMut(&Value, &Effect<Value>) -> Result<bool>,
    {
        self.slot_data = None;
        if let Some(ack) = &self.pending_ack {
            write_json(&self.profile.bridge_dir.join("ack.json"), ack)?;
            self.pending_ack = None;
        }
        let path = self.profile.bridge_dir.join("session.json");
        if !path.exists() {
            return Ok(());
        }
        let h: Handshake = read_json(&path)?;
        check_version(h.version)?;
        if h.contract != self.profile.contract
            || h.identity.game != self.profile.game
            || h.identity.save != self.profile.save
            || h.challenge.is_empty()
            || h.identity.seed.is_empty()
        {
            return Err("handshake mismatch".into());
        }
        if let Some(s) = &self.save {
            if s.identity != h.identity {
                return Err(
                    "refusing different seed/team/slot/save; use a separate bridge directory"
                        .into(),
                );
            }
        } else {
            self.commit(GameSave {
                version: VERSION,
                contract: h.contract.clone(),
                identity: h.identity.clone(),
                state: self.initial.clone(),
                receipts: BTreeMap::new(),
                checked_locations: BTreeSet::new(),
                goal: false,
            })?;
        }
        self.slot_data = h.slot_data.clone();
        let command_path = self.profile.bridge_dir.join("command.json");
        if command_path.exists() {
            let c: Command = read_json(&command_path)?;
            check_version(c.version)?;
            // Old boot/challenge commands are never executed after a restart.
            if c.challenge == h.challenge
                && c.boot == self.boot
                && c.identity == h.identity
                && self.last_attempt.as_ref() != Some(&c.attempt)
            {
                let s = self.save.as_ref().unwrap();
                let prior = s.receipts.get(&c.effect.id.0);
                let needs_reconcile = prior == Some(&c.effect)
                    && c.effect.kind == EffectKind::Permanent
                    && !observe(&s.state, &c.effect)?;
                let outcome = if let Some(old) = prior.filter(|_| !needs_reconcile) {
                    if old == &c.effect {
                        EffectOutcome::AlreadySatisfied
                    } else {
                        EffectOutcome::Rejected("effect ID reused with different payload".into())
                    }
                } else if !ready {
                    EffectOutcome::NotReady("game not ready; no effect committed".into())
                } else if !self
                    .profile
                    .items
                    .values()
                    .any(|m| m.kind == c.effect.kind && m.payload == c.effect.payload)
                {
                    EffectOutcome::Rejected("effect is outside the agreed profile".into())
                } else if prior.is_none() && s.receipts.len() >= MAX_RECEIPTS {
                    EffectOutcome::Rejected("receipt limit reached".into())
                } else {
                    match transaction(&s.state, &c.effect) {
                        Ok(state) => {
                            let verified = if c.effect.kind == EffectKind::Permanent {
                                observe(&state, &c.effect)
                            } else {
                                Ok(true)
                            };
                            match verified {
                                Ok(true) => {
                                    let mut next = s.clone();
                                    next.state = state;
                                    next.receipts
                                        .insert(c.effect.id.0.clone(), c.effect.clone());
                                    self.commit(next)?;
                                    EffectOutcome::Applied
                                }
                                Ok(false) => EffectOutcome::Rejected(
                                    "transaction did not establish the permanent state".into(),
                                ),
                                Err(reason) => EffectOutcome::Rejected(reason),
                            }
                        }
                        Err(reason) => EffectOutcome::Rejected(reason),
                    }
                };
                self.last_attempt = Some(c.attempt.clone());
                self.pending_ack = Some(Ack {
                    version: VERSION,
                    challenge: c.challenge,
                    boot: c.boot,
                    attempt: c.attempt,
                    identity: c.identity,
                    effect: c.effect,
                    outcome,
                });
                write_json(
                    &self.profile.bridge_dir.join("ack.json"),
                    self.pending_ack.as_ref().unwrap(),
                )?;
                self.pending_ack = None;
            }
        }
        self.heartbeat = self.heartbeat.checked_add(1).ok_or("heartbeat overflow")?;
        let s = self.save.as_ref().unwrap();
        let permanent_states = s
            .receipts
            .iter()
            .filter(|(_, e)| e.kind == EffectKind::Permanent)
            .map(|(id, e)| Ok((id.clone(), observe(&s.state, e)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        write_json(
            &self.profile.bridge_dir.join("snapshot.json"),
            &Snapshot {
                version: VERSION,
                contract: h.contract,
                challenge: h.challenge,
                identity: h.identity,
                boot: self.boot.clone(),
                heartbeat: self.heartbeat,
                capabilities: bridge_capabilities(),
                ready,
                checked_locations: s.checked_locations.clone(),
                goal: s.goal,
                receipts: s.receipts.clone(),
                permanent_states,
            },
        )
    }
    /// Commit a local gameplay state change (for example spending a resource)
    /// while preserving session, receipts, checks and goal. The closure is pure.
    pub fn update_state<F>(&mut self, update: F) -> Result<()>
    where
        F: FnOnce(&Value) -> Result<Value>,
    {
        let mut next = self.save.clone().ok_or("not enrolled")?;
        next.state = update(&next.state)?;
        self.commit(next)
    }
    /// Record only physically completed checks. Call from the game's check hook.
    pub fn report_check(&mut self, id: i64) -> Result<()> {
        let mut s = self.save.clone().ok_or("not enrolled")?;
        if !self.profile.locations.contains(&id) {
            return Err("unknown check".into());
        }
        s.checked_locations.insert(id);
        self.commit(s)
    }
    pub fn report_goal(&mut self) -> Result<()> {
        let mut s = self.save.clone().ok_or("not enrolled")?;
        s.goal = true;
        self.commit(s)
    }
}

#[cfg(test)]
mod tests;
