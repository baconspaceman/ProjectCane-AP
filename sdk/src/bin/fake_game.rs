//! Synthetic example only. No game process, game files or saves are accessed.
use std::{path::Path, time::Duration};
use universal_ap_bridge::{GameEndpoint, Profile};
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
        println!(
            "fake-game PROFILE.json [--ticks N] [--check ID] [--goal-after-item]\nSynthetic fixture only; writes only the profile bridge directory."
        );
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--version") {
        println!("fake-game {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let path = args
        .get(1)
        .ok_or("usage: fake-game PROFILE.json [--ticks N] [--check ID] [--goal-after-item]")?;
    let mut limit = None;
    let mut check = None;
    let mut goal = false;
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
            "--check" => {
                i += 1;
                check = Some(
                    args.get(i)
                        .ok_or("missing check ID")?
                        .parse::<i64>()
                        .map_err(|e| e.to_string())?,
                );
            }
            "--goal-after-item" => goal = true,
            _ => return Err("unknown argument".into()),
        }
        i += 1;
    }
    let profile = Profile::load(Path::new(path))?;
    let mut game = GameEndpoint::open(profile, serde_json::json!({"grants":0,"effects":[]}))?;
    let mut ticks = 0;
    loop {
        game.tick_with_observer(
            true,
            |state, e| {
                let mut next = state.clone();
                next["grants"] =
                    serde_json::json!(state["grants"].as_u64().ok_or("invalid fake state")? + 1);
                next["effects"]
                    .as_array_mut()
                    .ok_or("invalid fake effects")?
                    .push(e.payload.clone());
                Ok(next)
            },
            |state, e| {
                Ok(state["effects"]
                    .as_array()
                    .is_some_and(|effects| effects.contains(&e.payload)))
            },
        )?;
        if game.save().is_some()
            && let Some(id) = check.take()
        {
            game.report_check(id)?;
        }
        if goal
            && game
                .save()
                .is_some_and(|s| !s.receipts.is_empty() && !s.goal)
        {
            game.report_goal()?;
        }
        ticks += 1;
        if limit.is_some_and(|n| ticks >= n) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!(
        "Synthetic game stopped: {:?}",
        game.save().map(|s| &s.state)
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
