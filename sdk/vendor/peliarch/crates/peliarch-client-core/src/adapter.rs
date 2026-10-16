use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Every field must match before a host can deliver an effect. `save` is an
/// adapter-defined stable save identity, never a display name or process ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    pub game: String,
    pub seed: String,
    pub team: u32,
    pub slot: u32,
    pub save: String,
}

/// Missing evidence is distinct from negative evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observation<T> {
    Known(T),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Readiness {
    Ready,
    NotReady(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub locations: bool,
    pub goal: bool,
    pub death_link: bool,
    pub permanent_effects: bool,
    pub repeatable_effects: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterSnapshot {
    pub identity: Observation<SessionIdentity>,
    pub readiness: Readiness,
    pub checked_locations: Observation<BTreeSet<i64>>,
    pub goal: Observation<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EffectId(pub String);

impl EffectId {
    /// Stable within a session; AP received-item indices are zero-based.
    pub fn received_item(index: u64) -> Self {
        Self(format!("received:{index:020}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectKind {
    /// A desired persistent state the adapter can observe and reconcile.
    Permanent,
    /// A consumption or grant that must not be repeated after uncertainty.
    Repeatable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effect<P> {
    pub id: EffectId,
    pub kind: EffectKind,
    pub payload: P,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectOutcome {
    Applied,
    AlreadySatisfied,
    /// No side effect occurred. The host may retry when ready.
    NotReady(String),
    /// A terminal failure. The host parks this effect and advances others.
    Rejected(String),
    /// A side effect may have occurred. Repeatables are never auto-retried.
    Uncertain(String),
}

/// Narrow game seam. Implementations must bind each mutation to
/// `expected_session`, checking identity and readiness again at the mutation
/// boundary. A queued asynchronous command returns `Uncertain`, not `NotReady`.
/// Observing permanent effects must be safe and must not mutate the game.
pub trait GameAdapter {
    type Payload: Clone + Eq;

    fn capabilities(&self) -> Capabilities;
    fn snapshot(&mut self) -> AdapterSnapshot;
    fn observe_effect(&mut self, effect: &Effect<Self::Payload>) -> Observation<bool>;
    fn apply(
        &mut self,
        expected_session: &SessionIdentity,
        effect: &Effect<Self::Payload>,
    ) -> EffectOutcome;
}
