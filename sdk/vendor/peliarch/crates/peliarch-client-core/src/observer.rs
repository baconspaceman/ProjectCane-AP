//! Read-only AP connection for games whose existing client/server owns delivery.
//! This type exposes observations, never a mutable protocol client or a Host.
use std::collections::{BTreeMap, BTreeSet};

use archipelago_rs::{ConnectionOptions, Event, ItemHandling};

use crate::session::SessionRunner;

#[derive(Clone, Debug, PartialEq)]
pub struct ObserverSnapshot {
    pub seed: String,
    pub team: u32,
    pub slot: u32,
    pub slot_data: json::Value,
    /// All checks in this slot, from the server's checked/missing lists.
    pub locations: BTreeMap<i64, String>,
    pub checked: BTreeSet<i64>,
}

pub struct ObserverSession {
    session: SessionRunner,
}

impl ObserverSession {
    pub fn connect(
        endpoint: impl Into<String>,
        slot: impl Into<String>,
        game: impl Into<String>,
        password: Option<String>,
    ) -> Self {
        let options = ConnectionOptions::new()
            .receive_items(ItemHandling::None)
            .tags(["Tracker"]);
        let options = match password {
            Some(password) => options.password(password),
            None => options,
        };
        Self {
            session: SessionRunner::new(endpoint, slot, Some(game), options),
        }
    }

    pub fn update(&mut self) -> Vec<Event> {
        self.session.update()
    }

    pub fn is_connecting(&self) -> bool {
        self.session.is_connecting()
    }

    /// Fatal events contain `Error::Elsewhere`; the connection retains their cause.
    pub fn failure(&self) -> Option<&archipelago_rs::Error> {
        self.session.is_disconnected().then(|| self.session.err())
    }

    /// No cached facts survive a disconnect or a replacement connection.
    pub fn snapshot(&self) -> Option<ObserverSnapshot> {
        let client = self.session.client()?;
        let player = client.this_player();
        Some(ObserverSnapshot {
            seed: client.seed_name().into(),
            team: player.team(),
            slot: player.slot(),
            slot_data: client.slot_data().clone(),
            locations: client
                .server_checked_locations()
                .chain(client.unchecked_locations())
                .map(|location| (location.id(), location.name().to_string()))
                .collect(),
            checked: client
                .server_checked_locations()
                .map(|location| location.id())
                .collect(),
        })
    }

    pub fn disconnect(&mut self) {
        self.session.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn observer_wire_policy_and_reconnect_clear_old_slot_facts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            for attempt in 0..2 {
                let deadline = Instant::now() + Duration::from_secs(5);
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline);
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut socket =
                    tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
                let room = json::json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":8,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":8,"class":"Version"},"tags":[],"password":true,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":format!("seed-{attempt}"),"time":0.0}]);
                socket
                    .send(tungstenite::Message::Text(room.to_string().into()))
                    .unwrap();
                let packet: json::Value =
                    json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                assert_eq!(packet[0]["cmd"], "Connect");
                assert_eq!(packet[0]["items_handling"], 0);
                assert_eq!(packet[0]["tags"], json::json!(["Tracker"]));
                assert_eq!(packet[0]["slot_data"], true);
                assert_eq!(packet[0]["password"], "fixture-secret");
                let checked = if attempt == 0 { vec![1] } else { vec![] };
                let missing = if attempt == 0 { vec![2] } else { vec![3] };
                let connected = json::json!([{"cmd":"Connected","team":0,"slot":1,"players":[{"team":0,"slot":1,"alias":"slot","name":"slot"}],"missing_locations":missing,"checked_locations":checked,"slot_data":{"attempt":attempt},"slot_info":{"1":{"name":"slot","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]);
                socket
                    .send(tungstenite::Message::Text(connected.to_string().into()))
                    .unwrap();
                ready_tx.send(()).unwrap();
                advance_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                if attempt == 0 {
                    socket
                        .send(tungstenite::Message::Text(
                            json::json!([{"cmd":"RoomUpdate","checked_locations":[2]}])
                                .to_string()
                                .into(),
                        ))
                        .unwrap();
                    ready_tx.send(()).unwrap();
                    advance_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                // Disconnect is allowed; no checks, status, chat, item sync or storage writes.
                while let Ok(message) = socket.read() {
                    if message.is_close() {
                        break;
                    }
                    assert!(!message.is_text(), "unexpected observer command: {message}");
                }
            }
        });
        for attempt in 0..2 {
            let mut observer = ObserverSession::connect(
                &endpoint,
                "slot",
                "Fixture",
                Some("fixture-secret".into()),
            );
            assert!(observer.snapshot().is_none());
            let deadline = Instant::now() + Duration::from_secs(5);
            while observer.snapshot().is_none() {
                observer.update();
                assert!(Instant::now() < deadline, "observer failed to authenticate");
                thread::sleep(Duration::from_millis(1));
            }
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let facts = observer.snapshot().unwrap();
            assert_eq!(facts.seed, format!("seed-{attempt}"));
            assert_eq!(facts.slot_data["attempt"], attempt);
            assert_eq!(
                facts.checked,
                if attempt == 0 {
                    BTreeSet::from([1])
                } else {
                    BTreeSet::new()
                }
            );
            assert_eq!(facts.locations.len(), if attempt == 0 { 2 } else { 1 });
            advance_tx.send(()).unwrap();
            if attempt == 0 {
                ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                while observer.snapshot().unwrap().checked.len() < 2 {
                    observer.update();
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                advance_tx.send(()).unwrap();
            }
            observer.disconnect();
            assert!(observer.snapshot().is_none());
        }
        server.join().unwrap();
    }
}
