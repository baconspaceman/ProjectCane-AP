# Universal native frontend

The frontend consumes the same immutable `ClientSnapshot` and bounded `UiAction`
bridge as existing standalone renderers. It never owns a game process or performs
transport, journal or adapter calls on the render thread. Existing renderer APIs
remain available unchanged.

On Windows, a generic adapter bootstrap can call:

```rust,ignore
peliarch_client_gui::run_runner("My game · Archipelago", runner, adapter, store)?;
```

This starts `RunnerWorker<G, S>` on a dedicated worker thread and runs the native
window on the calling thread. The worker uses the runner's configured item policy,
session validation and durable recovery implementation. Closing the window stops
the worker. Existing game clients can instead keep their worker and call
`spawn(title, host_endpoint)`; Windows permits its event loop on a background
thread.

Connection, readiness, seed/save identity, confirmed checks, queued/completed and
parked effects, grouped unchecked locations, and selectable activity logs are
shown together. Recovery requires selecting the current blocked record and
explicitly accepting duplicate delivery risk before `RetryBlocked` is sent.
Chat uses `SubmitCommand("say MESSAGE")`; reconnect uses
`SubmitCommand("reconnect")`. A separate Command field sends raw commands to
existing native client handlers. The generic worker also accepts
`retry INDEX CONFIRM`; recovery commands without `CONFIRM` are refused. Other
unsupported commands produce an action refusal.

`cargo test -p peliarch-client-gui` tests input validation, recovery confirmation,
stale worker detection, outcome log deduplication, worker action handling and egui
tessellation. `cargo run -p peliarch-client-gui --example native_smoke` opens a
real synthetic fixture window, captures its framebuffer to
`peliarch-gui-smoke.png`, and closes. Set `PELIARCH_SMOKE_OUTPUT` to choose its
destination. This verifies native rendering without launching a game or granting
effects.

The shared window also renders the existing `TrackerView`: per-region counts,
supplied in-logic and quest locations, missing requirements, and revealed hints.
Workers supply the logic; the renderer does not infer it. Missing tracker data
leaves counts and the unchecked list visible with reachability marked unavailable.
The standalone hint action is offered only for a standalone tracker session.
See [the recomp programme](../../docs/recomp/PLAN.md) for the DKR backend and the
existing-APWorld target queue.

## Read-only AP observers

`observer::ObserverWorker<P>` connects with the `Tracker` tag and
`items_handling=0` through the core's `ObserverSession`. Use it when an existing
game client or server already owns checks and delivery. `P: ObserverTracker`
projects the server's slot data and checked IDs into `TrackerView`. It has no
game handle, Host, receipt journal, or mutable protocol client. `()` is the
counts-only fallback.

Create the worker with `ObserverWorker::new(game, provider)`, pass it to
`spawn_worker`, and use `run(title, endpoint)` for the same shared window.
Connection and reconnect controls work; delivery/recovery/chat controls are
hidden and their actions refused. `observer_only` defaults to false in older
serialized snapshots. A disconnect clears check/model facts. Inventory and
server goal status are currently unknown; the observer never turns an empty
inventory or a model's completion predicate into progress or victory.

Core socket tests cover the actual observer Connect packet, server check
updates and reconnect isolation. GUI tests check action refusal and that
unknown inventory does not display as zero reachable checks. These tests do
not establish a target game's gameplay compatibility.
