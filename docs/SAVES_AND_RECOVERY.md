# Saves, backups, and recovery

AP mode uses a separate profile rather than the normal ProjectCane save. Treat that profile as one unit.

For the portable profile layout, the important files are:

```text
profiles/<profile-name>/
  profile.json
  host-journal.json
  bridge/
    native-save.json
    native-save.json.bak
  user/
  logs/
```

`native-save.json` combines native game state and applied-item receipts. `host-journal.json` tracks the companion's side of delivery. The native `.bak` is a previous generation, not a substitute for a complete matched backup. Additional bridge/status files can appear during operation.

If you host a local solo server, preserve its matching multidata and server save too. In the development launcher, `seed.archipelago` is paired with `seed.apsave`. External-room server state belongs to that room's host.

## Back up a run

1. Save in the game and close it normally.
2. Wait for its launcher-owned helpers to finish shutting down.
3. Copy the **whole profile directory** into a dated backup folder.
4. Keep the matching game/client release version and, for local hosting, the paired server files.

Copying only the game's native state while leaving a newer item journal behind can create inconsistent recovery. Restoring a local backup also does not roll back an external AP server's checked locations or other players' progress.

## Continue versus start again

Resume the same profile to continue the same seed/slot. Use a separate profile for a genuinely new run. Reusing a server address does not prove that it still hosts the same seed.

Do not delete receipts, edit the save token, or repeatedly reconnect to obtain items again. The integration deliberately tracks deliveries so spent lives, charms, and already-applied items are not regranted on normal replay.

## If something fails

- Preserve the current profile before experimenting.
- Check F8's server and game status separately.
- If an AP helper stopped, save and close normally when practical; inspect the logs before restarting the same profile.
- If an update is incompatible with a save or extracted data, keep the old version recoverable. Replacing an executable is not a guaranteed save-format rollback.
- Do not restore a single `.bak` over the current native envelope without also reviewing its paired delivery/server state.

The first public release does not include a complete automatic recovery wizard or automatic updater. Deliberate, matched backups remain important.
