//! Shared AP session and legacy delivery seam. Scheduling and durable game
//! semantics remain explicit policies; both the portable host and existing
//! native adapters use this transport and the same unmodified wire history.
use std::collections::HashSet;

use archipelago_rs::{
    Client, Connection, ConnectionOptions, ConnectionState, ConnectionStateType, Error, Event,
    ReceivedItem,
};
use serde::de::DeserializeOwned;

pub struct SessionRunner<S: DeserializeOwned + Send + 'static = json::Value> {
    connection: Connection<S>,
}

pub type LegacyRunner<S> = SessionRunner<S>;

impl<S: DeserializeOwned + Send + 'static> SessionRunner<S> {
    pub fn new(
        endpoint: impl Into<String>,
        slot: impl Into<String>,
        game: Option<impl Into<String>>,
        options: ConnectionOptions,
    ) -> Self {
        let slot = slot.into();
        let game = game.map(Into::into);
        Self {
            connection: Connection::new(endpoint, slot.as_str(), game.as_deref(), options),
        }
    }

    pub fn update(&mut self) -> Vec<Event> {
        self.connection.update()
    }
    pub fn try_next_event(&mut self) -> Option<Event> {
        self.connection.try_next_event()
    }
    pub fn client(&self) -> Option<&Client<S>> {
        self.connection.client()
    }
    pub fn client_mut(&mut self) -> Option<&mut Client<S>> {
        self.connection.client_mut()
    }
    pub fn state(&self) -> &ConnectionState<S> {
        self.connection.state()
    }
    pub fn state_type(&self) -> ConnectionStateType {
        self.connection.state_type()
    }
    pub fn err(&self) -> &Error {
        self.connection.err()
    }
    pub fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }
    pub fn is_connecting(&self) -> bool {
        self.connection.is_connecting()
    }
    pub fn is_disconnected(&self) -> bool {
        self.connection.is_disconnected()
    }
    pub fn disconnect(&mut self) {
        *self.connection.state_mut() = ConnectionState::Disconnected(Error::ClientDisconnected);
    }
}

/// Legacy adapters keep their prior receipts, recovery rules and delivery
/// outputs. The runner supplies authoritative server checks and explicit AP
/// indices; it never deduplicates, renumbers or rewrites their item history.
pub trait LegacyDelivery {
    type Error;
    type ItemOutput;
    fn poll_locations(&mut self, checked: &HashSet<i64>) -> Result<Vec<i64>, Self::Error>;
    fn poll_items(
        &mut self,
        items: &[ReceivedItem],
        lane_available: bool,
    ) -> Result<Self::ItemOutput, Self::Error>;
}

pub type LegacyCheckResult<E> = (HashSet<i64>, Result<Vec<i64>, E>);

pub fn poll_legacy_checks<S, D>(client: &Client<S>, backend: &mut D) -> LegacyCheckResult<D::Error>
where
    S: DeserializeOwned + Send + 'static,
    D: LegacyDelivery,
{
    let confirmed = client
        .server_checked_locations()
        .map(|location| location.id())
        .collect();
    let result = backend.poll_locations(&confirmed);
    (confirmed, result)
}

pub fn poll_legacy_items<S, D>(
    client: &Client<S>,
    backend: &mut D,
    lane_available: bool,
) -> Result<D::ItemOutput, D::Error>
where
    S: DeserializeOwned + Send + 'static,
    D: LegacyDelivery,
{
    backend.poll_items(client.received_items(), lane_available)
}
