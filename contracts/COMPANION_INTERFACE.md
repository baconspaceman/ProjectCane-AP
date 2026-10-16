# Sly native UI and read-only logic interface v1

The SDK v1 files and exact Profile schema remain authoritative. Profile `save`
is a stable run ID. Each item mapping uses `kind: Repeatable` and
`payload: {item_id: <AP ID>}`: each indexed item counts exactly once, including
progressive copies. The native endpoint restores permanent state from receipts.
No Python connection or game mutation is allowed.

All files below live under Profile.bridge_dir. Trusted local per-user access only;
the profile creator restricts Windows ACLs. Writers publish using atomic replace.
JSON files are bounded to 1 MiB. Native UI must ignore stale status (>5 seconds).

- `ui-status.json`, companion writer: version, revision, updated_unix_ms, identity
  (SDK identity or null), save, endpoint, slot_name, server_connected, game_ready,
  connection_enabled, diagnostic, tracker (current matching logic output or null),
  players [{team,slot,name,game}], deliveries [{id,state}], capabilities {links:false,
  ghosts:false}, control_results [{id,ok,error}]. No credentials.
- `ui-events.json`, companion writer: {version:1,identity,events:[{id,kind,text,item_name,
  sender,flags}]}. Recent 100 events, no chat. flags retain AP progression/useful/trap
  bits (1/2/4). Native UI deduplicates by id.
- `controls/<unique-id>.json`, native UI writer: {version:1,id,save,action,
  endpoint?,slot_name?,password?}. Actions connect/disconnect/reconnect only.
  Password can exist transiently in the user-restricted request; companion removes
  the request before execution, never serializes it into status or logs. Native
  UI clears password input after submission. Interrupted requests require the
  user to retry; they are not silently replayed from an archive. A connect changes
  endpoint/credentials, not the paired save identity. New seed/slot uses new profile.
  There is no chat command. Disconnect disables automatic reconnection.
- `logic-input.json`, companion writer: {version:1,revision,identity,connected,
  history_ready,slot_data,received_items:[{index,item_id,item_name,location_id,
  player,flags}],checked_locations:[IDs],missing_locations:[IDs],game_facts:{
  observed_events:[names],events_complete:bool}}. Checked IDs are server-confirmed.
  Revision changes only when semantic data changes. Received history is complete,
  never a second grant stream. False history_ready/connected => unknown logic.
- `logic-output.json`, Python writer: version/revision/identity must echo input;
  status known|unknown, reason, bk bool|null, checks [{id,name,area,status,
  requirements}]. Companion rejects mismatched revision/identity. UI must not use
  an old output after disconnect/profile switch.
- `game-facts.json`, native writer: {version:1,identity,challenge,boot,
  observed_events:["Beat Raleigh",...],events_complete:true}. Companion accepts
  only alongside a live ready SDK snapshot with matching identity/challenge/boot.
- `active-locations.json`, companion writer: {version:1,identity,challenge,
  locations:[IDs]}. Native intersects saved physical checks with this matching
  authenticated seed's location set before publishing SDK snapshot checks.

SDK session.json includes validated flat upstream Sly slot_data. The companion
checks the 20 option encodings, Seed equality and optional bridge fields. The
active location set is restricted to the authenticated seed before reporting.

Current limitations: no links, ghost transport, profile switching in-process,
secure persistent password store, updater, or full notification personalization.
These are unfinished features, never implied by true baseline capabilities.

## Received inventory projection (additive)

`ui-status.json` also carries `inventory_status: known|unknown` and `inventory`
rows `{item_id,name,count,received_count,applied_count,pending_count,flags,
classification_source}`. `count` aliases received_count. These are totals from
the current authoritative indexed AP history, never the current amount of lives,
charms, or other spendable inventory. Applied means the SDK has a Completed
delivery record; label it **Delivered**, not **Currently held**. Pending includes
parked failures; use the existing delivery records for details.

The entire history is recomputed; replay cannot increment totals. Duplicate or
gapped indexes suppress known inventory. Disconnected or incomplete history gives
unknown rather than displaying stale numbers as current.

AP flags remain authoritative for actual location items and outgoing remote-game
items. Only the reserved starting-inventory source (location -2, sender slot 0,
flags 0) may use the pinned Sly item catalogue classification. That fallback is
labeled `local_precollected_catalog`; unknown IDs remain `unknown_precollected`.
No item-name inference or cross-game classification guessing occurs. The logic
input preserves `server_flags` separately alongside display flags and provenance.
