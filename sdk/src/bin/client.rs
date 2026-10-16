use peliarch_client_core::{
    connection::ConnectionProfile,
    runner::{Runner, RunnerConfig},
    store::JournalPath,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use universal_ap_bridge::{EffectId, FileAdapter, Profile};
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
        println!(
            "universal-ap-client PROFILE.json [--ticks N] [--retry EFFECT_ID]\nConnects the configured game bridge to Archipelago. AP_PASSWORD supplies the optional password.\n--retry explicitly authorizes retry of a parked effect; game receipts prevent duplicates only with a correct transactional adapter."
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--version") {
        println!(
            "universal-ap-client {} bridge protocol {}",
            env!("CARGO_PKG_VERSION"),
            universal_ap_bridge::VERSION
        );
        return Ok(());
    }
    let path = args
        .get(1)
        .ok_or("usage: universal-ap-client PROFILE.json [--ticks N] [--retry EFFECT_ID]")?;
    let mut limit = None;
    let mut retry = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--ticks" => {
                i += 1;
                limit = Some(
                    args.get(i)
                        .ok_or("missing ticks")?
                        .parse::<u64>()
                        .map_err(|e| e.to_string())?,
                );
            }
            "--retry" => {
                i += 1;
                retry = Some(EffectId(args.get(i).ok_or("missing effect ID")?.clone()));
            }
            _ => return Err("unknown argument".into()),
        }
        i += 1;
    }
    let p = Profile::load(Path::new(path))?;
    let mut adapter = FileAdapter::open(p.clone())?;
    let mut store = JournalPath::<serde_json::Value>::new(&p.journal_path);
    let mut runner = Runner::new(
        RunnerConfig {
            endpoint: p.endpoint,
            slot_name: p.slot_name,
            game: p.game,
            profile: ConnectionProfile::new(true, std::env::var("AP_PASSWORD").ok()),
            reconnect_delay: Duration::from_secs(2),
            report_interval: Duration::from_millis(250),
        },
        Instant::now(),
    );
    let mut last = String::new();
    let mut ticks = 0;
    loop {
        let status = match runner.poll(Instant::now(), &mut adapter, &mut store) {
            Ok(r) => {
                if let Some(id) = retry.as_ref()
                    && runner.host().is_some()
                {
                    runner.retry(id, true, &mut store)?;
                    println!("Explicit retry authorized for {}", id.0);
                    retry = None;
                }
                format!(
                    "connected={} diagnostic={:?} game={:?} deliveries={:?}",
                    r.connected,
                    r.diagnostic,
                    r.step.as_ref().map(|s| &s.snapshot.readiness),
                    runner.host().map(|h| h
                        .records()
                        .map(|r| (&r.effect.id, &r.state))
                        .collect::<Vec<_>>())
                )
            }
            Err(reason) => format!("paused: {reason}"),
        };
        if status != last {
            println!("{status}");
            last = status;
        }
        ticks += 1;
        if limit.is_some_and(|n| ticks >= n) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    runner.disconnect();
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
