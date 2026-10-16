# Roadmap

This is a development plan, not a delivery-date promise. The public release scope below supersedes earlier internal plans that placed every selected feature in RC1.

For the accepted defaults, optional behaviors, exclusions, and unresolved details, read the [complete planned feature contract](docs/FUTURE_FEATURES.md). It describes future targets rather than claiming those features already exist in RC1.

## RC1 — current playable native integration

The first public release provides the current Windows companion/runtime and source-assisted local native build, in-game connection controls, notifications, tracker/maps, logic-based BK status, basic accessibility controls, and isolated AP saves. The APWorld remains separate. RC1 does not distribute a prebuilt ProjectCane game or extraction-tool binary.

RC1 has known completion-notification and large-delivery responsiveness issues. Publishing it as RC1 does not imply these issues are fixed or that every game option has completed physical acceptance. The [known issues](docs/KNOWN_ISSUES.md) and [testing record](docs/TESTING.md) describe that boundary.

## RC1.5 — multiplayer presence and remaining selected features

Priorities include:

- A clear, durable goal-completion display and responsive status/UI during large release or collection batches.
- The complete notification filters, per-event overrides, optional sound cues, timing/scene behavior, and presentation presets.
- Expanded tracker guidance, verified missing-requirement explanations, input customization, and profile preferences.
- DeathLink, RingLink, EnergyLink, DamageLink, and TrapLink, with first-session review, supported mappings, and independent controls. GiftLink is excluded.
- A compact completion summary with play time, elapsed time, and progression contributions.
- Better profile/setup/update flows, explicit backup/rollback handling, and the remaining planned usability features.
- Cosmetic Sly ghosts: independent worlds, no collision or interaction, always silent, per-player visibility, optional sharing, and roster-only behavior in special sections.

Ghosts are visual presence, not shared-world co-op. The network/scene compatibility and save independence must be proven before they are presented as ready. Existing separate AP links will remain responsible for shared effects such as deaths.

The optional bundled solo example is deferred to **RC1.5**, not removed. The public RC1 package must not contain a developer's played save or privately generated room. It is not a practice sandbox or a promise of arbitrary in-game generation.

## RC2 — a later ProjectCane-based build

RC2 will be reconsidered as ProjectCane advances. A newer upstream build may change data extraction, engine behavior, or native hooks; it is not automatically compatible with an older AP client/save.

Broader upstream mod support could make future feature integration easier, if and when it becomes available. This project does not promise a ProjectCane mod API, an upstream delivery date, or that today's custom build can become a drop-in plugin without further work.

## Later or conditional

- Linux after the core integration is reliable; Steam Deck belongs to that effort, with separate handheld controls/readability/Game Mode checks.
- macOS only if appropriate ProjectCane support becomes available.
- An installer if it adds value beyond the portable ZIP.
- Practice mode after the main AP experience is reliable.
- A short, skippable Sly-specific walkthrough remains tentative; accessibility controls and reusable help matter first.

## Validation sequence

Development continues through short solo tests, one player operating multiple AP games, small groups on different games, and finally two Sly players exercising ghosts. Test seeds should use genuine random placements, supported non-minigame checks, and a short goal. No fixed item route or plando is required to demonstrate the integration.

A short goal does not guarantee a fixed finish time. Random item placement remains part of the game.
