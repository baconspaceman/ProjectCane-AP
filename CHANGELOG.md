# Changelog

## RC1 — Experimental native integration

The first public release of Project Kane AP, built on ProjectCane's native
Sly Cooper and the Thievius Raccoonus port.

- Native checks, item delivery and real boss/goal reporting through the Universal
  Archipelago SDK/Peliarch companion.
- Separate AP save profiles, durable delivery records, reconnect and resume.
- In-game connection panel, notifications, item inventory, quick/full trackers,
  map artwork, accessibility controls and an FPS/frame-time graph.
- Episode first-entry handling and direct hub access after the first matching key.
- Routine status publication moved off the game thread after gameplay feedback.
- Pinned local native build workflow and relocated Python/client runtime.

### Known release limitations

- Source-first distribution: the native game must be built locally. The ZIP does
  not contain ProjectCane or FFmpeg binaries, an extractor, or original game data.
- Goal completion can succeed without a dedicated in-game completion message.
- Large release/collection bursts can delay status updates and overflow the short
  recent-event list. This does not mean the saved goal was lost.
- Cosmetic ghosts, AP links and the rest of the expanded feature plan target RC1.5.
- The completed ending test used six supplied keys; it was not an unassisted
  randomized campaign. See [testing](docs/TESTING.md).

See the [roadmap](ROADMAP.md) and [known issues](docs/KNOWN_ISSUES.md).
