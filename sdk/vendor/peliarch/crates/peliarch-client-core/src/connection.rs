//! Shared Archipelago handshake policy for standalone and injected clients.
//!
//! Game adapters still own slot-data validation and delivery. A fresh socket must start without
//! link tags: those depend on validated slot data and are reconciled by the existing game loop.

use archipelago_rs::{ConnectionOptions, ItemHandling};

/// Reusable connection options, rebuilt for each attempt without changing retry timing.
///
/// Passwords remain runtime configuration, never part of a seed or persisted delivery state.
/// This type deliberately does not implement `Debug` or serialization.
#[derive(Clone)]
pub struct ConnectionProfile {
    own_world: bool,
    password: Option<String>,
}

impl ConnectionProfile {
    /// Receive other worlds and starting inventory, with own-world receipt chosen by the game.
    pub fn new(own_world: bool, password: Option<String>) -> Self {
        Self {
            own_world,
            password,
        }
    }

    /// Replace runtime credentials without changing the adapter's receipt policy.
    pub fn with_password(&self, password: Option<String>) -> Self {
        Self {
            own_world: self.own_world,
            password,
        }
    }

    /// Build the same untagged options for every reconnect. In particular, even an explicitly
    /// empty password is preserved rather than treated as missing.
    pub fn options(&self) -> ConnectionOptions {
        let options = ConnectionOptions::new().receive_items(ItemHandling::OtherWorlds {
            own_world: self.own_world,
            starting_inventory: true,
        });
        match self.password.as_deref() {
            Some(password) => options.password(password),
            None => options,
        }
    }
}

/// Whether a configured endpoint is worth opening a socket to.
///
/// This retains the legacy shared client's deliberately narrow test: empty URLs and placeholder
/// ports prompt for configuration, while socket URL parsing still belongs to `archipelago_rs`.
/// No normalization is applied to the value eventually passed to the connection.
pub fn is_connectable(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }
    let host_part = url
        .split_once("://")
        .map(|(_scheme, rest)| rest)
        .unwrap_or(url);
    match host_part.rsplit_once(':') {
        Some((_host, port)) => !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc::{self, TryRecvError};
    use std::thread;
    use std::time::{Duration, Instant};

    /// Read the real Connect packet without authenticating or asking the client to deliver
    /// anything. Empty package metadata avoids both filesystem writes and external services.
    fn capture_connect(options: ConnectionOptions) -> json::Value {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let (sender, receiver) = mpsc::channel();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "client never opened its socket");
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("accepting test connection: {error}"),
                }
            };
            // Winsock inherits the listener's nonblocking mode on accepted sockets.
            // The test server uses bounded blocking I/O for its handshake and packet capture.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket =
                tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
            let room = json::json!([{
                "cmd": "RoomInfo",
                "version": {"major": 0, "minor": 6, "build": 0, "class": "Version"},
                "generator_version": {"major": 0, "minor": 6, "build": 0, "class": "Version"},
                "tags": [],
                "password": false,
                "permissions": {"release": 0, "collect": 0, "remaining": 0},
                "hint_cost": 10,
                "location_check_points": 1,
                "games": [],
                "datapackage_checksums": {},
                "seed_name": "previous-seed",
                "time": 0.0
            }]);
            socket
                .send(tungstenite::Message::Text(room.to_string().into()))
                .unwrap();
            let packet = socket.read().unwrap();
            let connect: json::Value = json::from_str(packet.to_text().unwrap()).unwrap();
            sender.send(connect[0].clone()).unwrap();
        });
        let mut connection = archipelago_rs::Connection::<json::Value>::new(
            format!("ws://{address}"),
            "previous-slot",
            Some("fixture-game"),
            options,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let connect = loop {
            connection.update();
            match receiver.try_recv() {
                Ok(connect) => break connect,
                Err(TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "client never sent Connect");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(TryRecvError::Disconnected) => {
                    server.join().expect("test server failed");
                    panic!("test server did not capture Connect");
                }
            }
        };
        server.join().unwrap();
        connect
    }

    #[test]
    fn extracted_profile_sends_the_previous_handshake_unchanged() {
        for own_world in [true, false] {
            for password in [None, Some(""), Some("previous-password")] {
                // Literal pre-extraction BB/ER options builder. Comparing actual packets catches
                // a dropped empty password, changed receive flags or prematurely advertised tag.
                let legacy = ConnectionOptions::new().receive_items(ItemHandling::OtherWorlds {
                    own_world,
                    starting_inventory: true,
                });
                let legacy = match password {
                    Some(password) => legacy.password(password),
                    None => legacy,
                };
                let profile = ConnectionProfile::new(own_world, password.map(str::to_owned));
                let before = capture_connect(legacy);
                let after = capture_connect(profile.options());
                assert_eq!(after, before);
                assert_eq!(after["items_handling"], if own_world { 7 } else { 5 });
                assert_eq!(after["tags"], json::json!([]));
                assert_eq!(after["password"], json::to_value(password).unwrap());
                assert_eq!(after["slot_data"], true);
            }
        }
    }

    #[test]
    fn legacy_endpoint_shapes_keep_their_configuration_decision() {
        for endpoint in ["", "   ", "archipelago.gg:PORT", "host:", "host:123x"] {
            assert!(!is_connectable(endpoint), "{endpoint}");
        }
        for endpoint in [
            "archipelago.gg:12345",
            "ws://localhost:38281",
            "wss://archipelago.gg",
            "[::1]:38281",
            "192.168.0.10:38281",
        ] {
            assert!(is_connectable(endpoint), "{endpoint}");
        }
    }
}
