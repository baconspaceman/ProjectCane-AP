//! A slow authentication response must not become DeathLink clock skew.
use std::{
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};

use crate::{Client, ConnectionOptions, DeathLinkOptions, Event};

fn timestamp(time: SystemTime) -> f64 {
    time.duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

fn server_time(local: SystemTime, ahead: bool) -> SystemTime {
    if ahead {
        local + Duration::from_secs(30)
    } else {
        local - Duration::from_secs(30)
    }
}

fn delayed_login(ahead: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("ws://{}", listener.local_addr().unwrap());
    let (sent_tx, sent_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        let mut socket = tungstenite::accept_with_config(stream, Some(Default::default())).unwrap();
        let room = json!([{"cmd":"RoomInfo","version":{"major":0,"minor":6,"build":8,"class":"Version"},"generator_version":{"major":0,"minor":6,"build":8,"class":"Version"},"tags":[],"password":false,"permissions":{"release":0,"collect":0,"remaining":0},"hint_cost":10,"location_check_points":1,"games":["Fixture"],"datapackage_checksums":{},"seed_name":"clock-test","time":timestamp(server_time(SystemTime::now(),ahead))}]);
        socket
            .send(tungstenite::Message::Text(room.to_string().into()))
            .unwrap();
        let connect: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(connect[0]["cmd"], "Connect");
        // Deliberately exceed clients' one-second freshness allowance. The
        // clock sample has already arrived and must not include this delay.
        thread::sleep(Duration::from_secs(2));
        let connected = json!([{"cmd":"Connected","team":0,"slot":1,"players":[{"team":0,"slot":1,"alias":"slot","name":"slot"}],"missing_locations":[],"checked_locations":[],"slot_data":{},"slot_info":{"1":{"name":"slot","game":"Fixture","type":1,"group_members":[]}},"hint_points":0}]);
        socket
            .send(tungstenite::Message::Text(connected.to_string().into()))
            .unwrap();
        let sent_at = SystemTime::now();
        socket.send(tungstenite::Message::Text(json!([{"cmd":"Bounced","tags":["DeathLink"],"data":{"time":timestamp(server_time(sent_at,ahead)),"source":"peer","cause":"fixture"}}]).to_string().into())).unwrap();
        sent_tx.send(sent_at).unwrap();
        let outgoing: Value =
            serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(outgoing[0]["cmd"], "Bounce");
        let actual = outgoing[0]["data"]["time"].as_f64().unwrap();
        let expected = timestamp(server_time(SystemTime::now(), ahead));
        assert!(
            (actual - expected).abs() < 0.5,
            "outgoing timestamp skew: {}s",
            actual - expected
        );
    });
    let mut client: Client<Value> = smol::block_on(Client::connect(
        endpoint,
        "slot",
        Some("Fixture"),
        ConnectionOptions::new().tags(["DeathLink"]),
    ))
    .unwrap();
    let expected = sent_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match client.try_next_event() {
            Some(Event::DeathLink {
                time,
                server_time: raw,
                ..
            }) => {
                let error = timestamp(time) - timestamp(expected);
                assert!(error.abs() < 0.5, "incoming timestamp skew: {error}s");
                assert!((timestamp(raw) - timestamp(server_time(expected, ahead))).abs() < 0.001);
                break;
            }
            Some(_) => panic!("unexpected non-DeathLink event"),
            None => {
                assert!(Instant::now() < deadline, "missing incoming DeathLink");
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
    client
        .death_link(DeathLinkOptions::new().source("slot".into()))
        .unwrap();
    server.join().unwrap();
}

#[test]
fn delayed_login_with_server_clock_ahead_keeps_deathlinks_fresh() {
    delayed_login(true);
}

#[test]
fn delayed_login_with_server_clock_behind_keeps_deathlinks_fresh() {
    delayed_login(false);
}

#[test]
fn deathlink_wire_times_preserve_distinct_subseconds_and_accept_integer_peers() {
    for seconds in [1_792_000_000.125, 1_792_000_000.875, 1_792_000_000.0] {
        let link: crate::protocol::DeathLink =
            serde_json::from_value(json!({"time": seconds, "source": "peer", "cause": null}))
                .unwrap();
        assert_eq!(timestamp(link.time), seconds);
        assert_eq!(serde_json::to_value(link).unwrap()["time"], json!(seconds));
    }
    let integer: crate::protocol::DeathLink =
        serde_json::from_value(json!({"time": 1_792_000_000_u64, "source": "peer", "cause": null}))
            .unwrap();
    assert_eq!(timestamp(integer.time), 1_792_000_000.0);
}
