# Architecture

ProjectCane AP follows the native game/companion model demonstrated by **4laric's Sunshine integration**, adapted to Sly's existing Python world logic.

| Component | Authority |
|---|---|
| Sly APWorld | Original item/check identities, generation options, placement rules, and goal logic; additive native metadata/preferences. |
| Universal SDK / Rust-Peliarch companion | The single AP networking and delivery authority, check reporting, item history, and goal reporting. |
| Python Sly logic worker | Read-only evaluation of actual Sly world rules for availability and BK status. |
| ProjectCane C++ adapter | Physical checks, actual native effects, safe-state application, and persisted game/item receipts. |
| Native UI | Connection controls, notifications, tracker/map presentation, input focus, and available display settings. |

Python does not run a second CommonClient connection that independently grants items. The native UI controls the companion through structured local messages; it does not scrape a terminal log.

## Collection and receipt are different events

A physical stage key or bottle can complete a location check. The reward at that location comes from the randomized AP placement and may belong to another player. Receiving an item must not recursively report the original location as collected.

The native adapter therefore preserves physical collection flags separately from received key/ability/bottle quantities. Saves pair those effects with delivery receipts. Replayed network history is not treated as permission to refill spent consumables.

## Tracker truth

The worker reconstructs the pinned world's region/rule graph from validated slot data, actual received inventory, and actual completed/observed events. It evaluates the whole session location set for BK status. Merely reaching a boss does not count as defeating it.

Unavailable explanations are precise only where implemented and verified. Otherwise the UI uses an honest fallback. Stale, mismatched, or incomplete state is unknown, never a guessed BK result.

## Native build boundary

The current implementation modifies ProjectCane source hooks. It is not a plugin for an existing official mod API. Original disc assets remain player-supplied. Source patches, binaries, and third-party components must be distributed only according to their applicable permissions; the architecture does not determine redistribution rights.

Future ghosts use a separate cosmetic presence/pose mechanism. They must not be implemented as AP item deliveries, share inventory/save state, or gain authority over another player's world.
