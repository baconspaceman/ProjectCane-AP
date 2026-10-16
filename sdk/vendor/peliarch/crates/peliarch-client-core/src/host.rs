use std::{collections::BTreeMap, error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    AdapterSnapshot, Effect, EffectId, EffectKind, EffectOutcome, GameAdapter, Observation,
    Readiness, SessionIdentity,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryState {
    Pending,
    Completed,
    Rejected(String),
    /// Also used as the durable write-ahead state before adapter mutation.
    Uncertain(String),
}

/// An optional persistence representation for NEW adapters. Legacy clients
/// retain their existing ledgers; this is not a mandatory universal disk schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRecord<P> {
    pub effect: Effect<P>,
    pub state: DeliveryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSnapshot<P> {
    pub identity: SessionIdentity,
    pub records: Vec<DeliveryRecord<P>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    IdentityMismatch,
    ConflictingEffect(EffectId),
    UnknownEffect(EffectId),
    NotParked(EffectId),
    Checkpoint(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentityMismatch => write!(f, "adapter session does not match host session"),
            Self::ConflictingEffect(id) => {
                write!(f, "effect ID {} reused with different content", id.0)
            }
            Self::UnknownEffect(id) => write!(f, "unknown effect {}", id.0),
            Self::NotParked(id) => write!(f, "effect {} is not parked", id.0),
            Self::Checkpoint(reason) => write!(f, "effect checkpoint failed: {reason}"),
        }
    }
}

impl Error for HostError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectReport {
    pub id: EffectId,
    pub outcome: EffectOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    pub snapshot: AdapterSnapshot,
    pub effects: Vec<EffectReport>,
}

/// Session-bound host. Protocol connections feed mapped effects into `enqueue`;
/// the adapter supplies observations. No socket, game, UI or Windows assumptions.
pub struct Host<P> {
    identity: SessionIdentity,
    records: BTreeMap<EffectId, DeliveryRecord<P>>,
}

impl<P: Clone + Eq> Host<P> {
    pub fn new(identity: SessionIdentity) -> Self {
        Self {
            identity,
            records: BTreeMap::new(),
        }
    }

    /// Reject journals belonging to another game, seed, team, slot or save.
    pub fn restore(
        identity: SessionIdentity,
        snapshot: HostSnapshot<P>,
    ) -> Result<Self, HostError> {
        if identity != snapshot.identity {
            return Err(HostError::IdentityMismatch);
        }
        let mut host = Self::new(identity);
        for record in snapshot.records {
            if let Some(existing) = host.records.get(&record.effect.id) {
                if existing != &record {
                    return Err(HostError::ConflictingEffect(record.effect.id));
                }
            } else {
                host.records.insert(record.effect.id.clone(), record);
            }
        }
        Ok(host)
    }

    pub fn identity(&self) -> &SessionIdentity {
        &self.identity
    }

    pub fn records(&self) -> impl Iterator<Item = &DeliveryRecord<P>> {
        self.records.values()
    }

    pub fn export(&self) -> HostSnapshot<P> {
        HostSnapshot {
            identity: self.identity.clone(),
            records: self.records.values().cloned().collect(),
        }
    }

    /// Replay with identical content is a no-op, including parked outcomes.
    pub fn enqueue(&mut self, effect: Effect<P>) -> Result<(), HostError> {
        if let Some(existing) = self.records.get(&effect.id) {
            if existing.effect != effect {
                return Err(HostError::ConflictingEffect(effect.id));
            }
        } else {
            self.records.insert(
                effect.id.clone(),
                DeliveryRecord {
                    effect,
                    state: DeliveryState::Pending,
                },
            );
        }
        Ok(())
    }

    /// Explicit recovery only. Callers must obtain informed player confirmation
    /// before retrying an uncertain repeatable: it may duplicate a game effect.
    pub fn authorize_retry(&mut self, id: &EffectId) -> Result<(), HostError> {
        let record = self
            .records
            .get_mut(id)
            .ok_or_else(|| HostError::UnknownEffect(id.clone()))?;
        if !matches!(
            record.state,
            DeliveryState::Rejected(_) | DeliveryState::Uncertain(_)
        ) {
            return Err(HostError::NotParked(id.clone()));
        }
        record.state = DeliveryState::Pending;
        Ok(())
    }

    /// Read native progress without observing or executing delivery effects.
    /// The same identity, readiness and capability guards apply as in `step`.
    pub fn observe<A>(&self, adapter: &mut A) -> Result<AdapterSnapshot, HostError>
    where
        A: GameAdapter<Payload = P>,
    {
        let capabilities = adapter.capabilities();
        let mut snapshot = adapter.snapshot();
        // Unsupported observations must stay unknown even if an adapter happens
        // to return a value. They cannot become checks or victory reports.
        if !capabilities.locations {
            snapshot.checked_locations =
                Observation::Unknown("location observation unsupported".into());
        }
        if !capabilities.goal {
            snapshot.goal = Observation::Unknown("goal observation unsupported".into());
        }
        match &snapshot.identity {
            Observation::Known(identity) if identity != &self.identity => {
                return Err(HostError::IdentityMismatch);
            }
            Observation::Unknown(_) => {
                snapshot.checked_locations =
                    Observation::Unknown("session identity unknown".into());
                snapshot.goal = Observation::Unknown("session identity unknown".into());
                return Ok(snapshot);
            }
            _ => {}
        }
        if snapshot.readiness != Readiness::Ready {
            snapshot.checked_locations = Observation::Unknown("game not ready".into());
            snapshot.goal = Observation::Unknown("game not ready".into());
        }
        Ok(snapshot)
    }

    /// `checkpoint` MUST durably write the supplied record, bound to the supplied
    /// session, before returning success. It is invoked before every mutation.
    /// If a final checkpoint fails, the in-memory record remains uncertain and
    /// the durable write-ahead record prevents blind repeatable replay on restart.
    pub fn step<A, C>(
        &mut self,
        adapter: &mut A,
        checkpoint: &mut C,
    ) -> Result<StepReport, HostError>
    where
        A: GameAdapter<Payload = P>,
        C: FnMut(&SessionIdentity, &DeliveryRecord<P>) -> Result<(), String>,
    {
        let snapshot = self.observe(adapter)?;
        if !matches!(snapshot.identity, Observation::Known(_))
            || snapshot.readiness != Readiness::Ready
        {
            return Ok(StepReport {
                snapshot,
                effects: vec![],
            });
        }
        let capabilities = adapter.capabilities();
        let mut effects = Vec::new();
        for record in self.records.values_mut() {
            if matches!(record.state, DeliveryState::Rejected(_)) {
                continue;
            }
            if record.effect.kind == EffectKind::Repeatable
                && matches!(
                    record.state,
                    DeliveryState::Completed | DeliveryState::Uncertain(_)
                )
            {
                continue;
            }
            let supported = match record.effect.kind {
                EffectKind::Permanent => capabilities.permanent_effects,
                EffectKind::Repeatable => capabilities.repeatable_effects,
            };
            let outcome = if !supported {
                EffectOutcome::Rejected("effect kind unsupported by adapter".into())
            } else if record.effect.kind == EffectKind::Permanent {
                match adapter.observe_effect(&record.effect) {
                    Observation::Known(true) => EffectOutcome::AlreadySatisfied,
                    Observation::Unknown(reason) => {
                        effects.push(EffectReport {
                            id: record.effect.id.clone(),
                            outcome: EffectOutcome::NotReady(reason),
                        });
                        continue;
                    }
                    Observation::Known(false) => {
                        Self::execute(&self.identity, record, adapter, checkpoint)?
                    }
                }
            } else {
                Self::execute(&self.identity, record, adapter, checkpoint)?
            };
            let next = DeliveryRecord {
                effect: record.effect.clone(),
                state: match &outcome {
                    EffectOutcome::Applied | EffectOutcome::AlreadySatisfied => {
                        DeliveryState::Completed
                    }
                    EffectOutcome::NotReady(_) => DeliveryState::Pending,
                    EffectOutcome::Rejected(reason) => DeliveryState::Rejected(reason.clone()),
                    EffectOutcome::Uncertain(reason) => DeliveryState::Uncertain(reason.clone()),
                },
            };
            if next != *record {
                checkpoint(&self.identity, &next).map_err(HostError::Checkpoint)?;
                *record = next;
            }
            effects.push(EffectReport {
                id: record.effect.id.clone(),
                outcome,
            });
        }
        Ok(StepReport { snapshot, effects })
    }

    fn execute<A, C>(
        identity: &SessionIdentity,
        record: &mut DeliveryRecord<P>,
        adapter: &mut A,
        checkpoint: &mut C,
    ) -> Result<EffectOutcome, HostError>
    where
        A: GameAdapter<Payload = P>,
        C: FnMut(&SessionIdentity, &DeliveryRecord<P>) -> Result<(), String>,
    {
        let before = DeliveryRecord {
            effect: record.effect.clone(),
            state: DeliveryState::Uncertain("execution started; outcome not recorded".into()),
        };
        checkpoint(identity, &before).map_err(HostError::Checkpoint)?;
        *record = before;
        Ok(adapter.apply(identity, &record.effect))
    }
}
