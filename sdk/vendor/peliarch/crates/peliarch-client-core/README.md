# Portable client core

This crate provides the game adapter seam and a small session-bound host. It also
contains the existing client UI state/channel model and AP connection profile,
which legacy crates consume through their original APIs. Bloodborne and Elden
Ring retain their seed contracts and game-specific delivery ledgers.

## Shared runner and storage

New clients implement `runner::GameIntegration` alongside `GameAdapter` for
slot-data validation, save pairing and received-item mapping. `Runner::poll`
owns connection/reconnect, restores the session-bound host, and reports observed
checks and goal after slot validation and persistent save pairing. Native progress
can be reported before item history arrives: AP may omit empty `ReceivedItems`
packets at connect and on `Sync`, so initial checks must be able to create the
first item. Delivery and restored-effect reconciliation still wait for an
authoritative indexed replay (`ReceivedItems(0)`) on every connection. The runner
requests that replay with `Sync` after connecting when history is absent; an empty
local item vector never establishes an empty server history. It retries
checks until server confirmation and retains completed/parked delivery records
through reconnects. Journal effects absent from current server history halt
delivery with a diagnostic rather than replaying items from another history.

Use `store::JournalPath<Payload>` with the runner. It opens the journal after
the session is known. `DurableHostStore` validates version 1 JSON envelopes,
persists even empty session identities, holds a crash-released OS lock, flushes
staging data and atomically replaces the previous generation. Unsupported versions,
corruption, ownership conflicts and wrong sessions fail explicitly. Keep journals
on a local filesystem supporting these lock/replace operations. Windows tests
exercise actual process termination; physical power-loss behavior is not claimed.

Game-side effects still require their own durability mechanism. The low-level
`store::write_file_atomic` helper does not acquire a lock and cannot make an
external game API transactional. Its caller must hold exclusive writer ownership.
The neutral starter demonstrates separate game state and host receipt storage.

`RunnerReport` exposes protocol events, diagnostics and host step outcomes for a
headless application or UI. No graphical renderer or game attachment is required
by the runner. Existing native clients continue using their established loops.
While waiting for history, `step` contains guarded native observations with no
effect outcomes and `diagnostic` explains that item delivery is waiting.

## API and compatibility policy

The initial toolkit API is version 0.1.0. Freeze the exported source, dependency
lock and manifest together for each integration trial. Additive capabilities
must default to unsupported; existing seed fields and journal versions retain
their meaning. An incompatible adapter API requires a new SDK minor version;
an incompatible disk envelope requires an explicit load-time migration or a
clearly separate format. Never silently reset an old journal. These rules do not
replace the existing clients' stricter paired release compatibility rules.

## Adapter semantics

A new game implements `GameAdapter`: capabilities, readiness and identity,
checked locations and goal observations, permanent-effect observations, and
effect delivery. Unknown evidence is explicit. Adapters must verify the expected
game/seed/team/slot/save at the mutation boundary, including when an asynchronous
transport queues commands. Effect observations must refer to the identity in the
snapshot; if that identity changes while reading, return `Unknown`.

The protocol worker maps received items into `Effect<Adapter::Payload>` and calls
`Host::enqueue`. `EffectId::received_item(index)` supplies a stable AP index key.
Replaying identical content is harmless; reusing the same key with another
payload or effect kind is refused. `Host::step` returns observations and effect
outcomes to the worker. `Host::observe` reads the same guarded native snapshot
without observing, executing or updating delivery effects. Unknown identity or a
game that is not ready suppresses checks, goal and delivery. Capabilities also
suppress unsupported observations and park unsupported effects.

Each `step` requires a fallible checkpoint callback. A successful callback must
mean the record is durably stored for the supplied session. Before adapter
delivery, the host checkpoints an uncertain write-ahead record. This prevents a
crash between delivery and final acknowledgement from silently replaying a
repeatable effect. If the callback fails before delivery, delivery never starts;
if it fails afterward, uncertainty is retained. Hosts restore through the
session-bound `HostSnapshot` envelope and reject journals from another session.
This envelope is optional for new clients, not a new required legacy disk format.

Permanent effects represent observable, idempotent desired state. They reconcile
when the same save rolls back, and wait when observation is unknown. Repeatable
effects represent consumables or events: completed and uncertain records never
auto-retry. `authorize_retry` accepts only parked records; the calling UI must
obtain informed confirmation before retrying an uncertain repeatable. Exactly-once
delivery still requires game-side durable receipts; the host cannot infer whether
an unacknowledged command took effect.

`NotReady` means no effect occurred and can be retried automatically. An
asynchronous command that was written but not acknowledged returns `Uncertain`.
`Rejected` parks only that effect, allowing later items to advance.

Run the portable conformance and extracted-module tests with:

```text
cargo test -p peliarch-client-core
```
