# Planned features and defaults

This is the accepted design target for **RC1.5 and subsequent development**, not a list of features already present in RC1. Some basic controls exist today; their complete customization, multiplayer behavior, and acceptance still need work. Use the [RC1 setup guide](GETTING_STARTED.md), [known issues](KNOWN_ISSUES.md), and [roadmap](../ROADMAP.md) for current availability.

There are no promised delivery dates. Native behavior, peer compatibility, performance, and any dependencies on future ProjectCane work must be verified. This plan does not promise an upstream mod API or when one might arrive.

## Connection, profiles, and saves

| Feature | Planned default and choices |
|---|---|
| Main interface | Connect, Reconnect, Disconnect, and keyboard text entry inside the game. Controller navigation may supplement typing. No AP Port Studio requirement. |
| Connection loss | Continue gameplay and automatically reconnect, with manual reconnect and pause-on-disconnect optional. Deliberate Disconnect stops retries. |
| Named profiles | Remember server/slot combinations. Protected password storage optional. Automatic startup connection optional and **off by default**. |
| Save selection | Show a chooser with the matching seed/team/slot save highlighted. Automatic resume optional and **off by default**; ambiguous/new/mismatched runs still need a choice. |
| Save isolation | AP mode selects separate AP save storage automatically. A disconnect never causes fallback to the normal ProjectCane save. |
| Autosaves | Safe-point AP autosaves by default; checkpoint-based timing optional. Either mode must keep game effects and item receipts consistent and recoverable. Autosaving does not create a new respawn checkpoint. |
| Personal preferences | Shared accessibility, input, and presentation defaults, with optional per-profile overrides. Seed/check/save state and link approvals stay session-specific. |
| Troubleshooting | Compact server/game status, expandable plain-language help, and a manual local diagnostic export. No automatic uploads or secret-bearing reports. |

Offline checks need durable reconciliation after reconnect. Links and resource exchanges remain inactive while disconnected. Startup connection, shared preferences, or a reused server address must never bypass save identity checks or first-session review.

## Notifications

- **Scope:** only your activity by default—items received, items your checks send, and your local rewards. Other players' progression, goal completions, or all item activity are optional filters.
- **Detail:** compact by default; detailed messages and per-event overrides optional. Include sender/recipient and, when legitimately known, location provenance.
- **Timing:** compact messages start at **4 seconds**, detailed messages at **7 seconds**. Adjustable durations and per-event overrides are planned.
- **Placement:** **bottom-left** by default. Presets: bottom-left, top-left, top-right, bottom-right, center-right, center-left. Floating mode detaches the in-game panel for dragging and locking; Reset returns it to bottom-left. This is not a separate desktop window.
- **Sound:** **silent by default**, with optional category selection and volume. The preferred cue is a short stage-end-key sound from player-supplied assets, if suitable, or a quick chime. Avoid long fanfares, overlapping cues, and noisy bursts; the final sound needs listening review.
- **History:** popups plus a short recent client-activity list while the game is open. No persistent searchable notification archive, room-chat feed, chat box, or sending chat.
- **Focus:** ordinary notifications never pause gameplay or capture movement/typing input.

Cutscene defaults depend on the scene:

| Scene | Default |
|---|---|
| Ordinary in-engine scene | Show notifications normally. |
| Comic-book-style world/boss introduction | Hold notifications and summarize afterward. |
| Post-boss/world-transition cinematic | Hold notifications and summarize afterward. |
| A deferred cinematic is skipped | Release the summary when normal presentation is safe. |

Immediate, individual-afterward, and summary-afterward modes remain selectable. This changes presentation only: received items are recorded promptly, while unsafe gameplay effects wait for an appropriate game state.

## Accessibility and colors

Standard visuals remain the default. High contrast, reduced animation, separate text/UI sizing, alternate color-vision palettes, and custom semantic colors are planned across the AP menus, tracker, and notifications. These are interface requirements, not a claim to redesign every aspect of the base game's accessibility.

Default item categories apply to both received and sent items:

| AP category | Default color |
|---|---|
| Ordinary/filler | White |
| Progression | Green |
| Trap | Red |

Use the seed's authoritative item classification, not a fixed guess from its name. A heart container or mushroom can be progression in another world. Useful-only, combined, and unknown classifications need explicit handling; they are not silently treated as an already-decided new category.

Every meaning needs a text label and/or distinct icon as well as color. Alternate palettes are selectable; there is no single correction that suits every form of color-vision deficiency. Check availability/completion styling stays distinct from reward classification. Unknown rewards must not be revealed by inspecting hidden placements.

Built-in and personal presets cover **presentation only**. Applying a preset must not change a seed, activate a link, spend resources, change credentials, or select another save.

## Tracker, maps, and BK status

- Provide quick and full views, each usable with or without a map. Include items, checks, and unavailable-check explanations; an inventory display alone is insufficient.
- Use the actual Sly world rules, options, and enabled tricks. Show verified explanations where implemented; otherwise say that detailed requirements are unavailable. Never guess. If reachability itself is uncertain, show **Unknown**.
- Optional **“What can I do next?”** suggestions use verified available checks without promising an optimal route or revealing unknown rewards. Their exact initial presentation is still to be settled.
- Quick view follows Sly by default. Full view follows until another area is selected/pinned; **Follow Sly** resumes following. The two views have independent behavior. Pinning a map area does not mean pinning a gameplay objective.
- Show all enabled checks/requirements and known AP hints by default. Discovery-focused visibility is optional; known-hint visibility has its own toggle.
- Keep gameplay running by default. Quick view is passive and never pauses; a full-tracker pause preference is optional.
- Rebindable keyboard keys and controller combinations; hold for quick view, toggle for full view by default, with both behaviors adjustable.
- Full tracker opens in **view-first** mode. A separate action transfers focus to the tracker. Immediate focus and mouse-plus-controller interaction are optional alternatives. Text input must not also move Sly.
- Preserve separate external PopTracker compatibility work; a native map using its art does not prove the external application works.

**BK detection** evaluates the entire enabled world after pending state has been reconciled. On a verified transition with no available in-logic checks and the goal still incomplete, announce **“You are now BK'd.”** Do not infer BK from an empty current-map list, discovery filtering, stale data, or disconnection. Out-of-logic opportunities do not prevent a genuine BK result.

By default the tracker contains BK status. An optional persistent HUD notice stays onscreen only while verified blocked and disappears when a check becomes available. A filler item that opens nothing must not dismiss it. On unblocking, show a short **“You are no longer BK'd”** alert by default; an area/check-count breakdown or no alert are optional. Avoid repeated sounds and clear/suspend stale or completed-run BK presentation.

**Bookmarks, favorites, per-check private notes, and pinned objectives were removed from the accepted scope.** They are not hidden future promises. Area selection/pinning for map navigation remains.

## Cross-game links

All participation is optional. Seed-provided preferences appear in a **first-session in-game review before activation**. The player makes the final choice, remembered for that validated profile. Missing/new preferences start inactive. Players can change settings during a run without restarting/reconnecting; changes apply safely to future activity, not by undoing previous exchanges or damage.

| Link | Planned Sly behavior when enabled |
|---|---|
| **DeathLink** | Share actual deaths with participating games. Prevent echo loops and defer unsafe application during loading/cinematics. |
| **RingLink** | Ordinary coin gains by default; incoming shared losses optional. Clamp losses at zero. No loss of progression keys, bottles/checks, pages, or episode unlocks. Distinguish genuine changes from rollover, conversion, or reload. |
| **EnergyLink** | Lives and charms, with manual deposits/withdrawals by default and automatic exchange optional. Enabling EnergyLink alone never enables automation. Conversion rates, reserves, limits, and recoverable shared accounting still need definition. |
| **DamageLink** | Charms-only by default: remove a charm if present, otherwise no effect. Full damage is optional and can kill an unprotected Sly. Coordinate with DeathLink to avoid duplicate punishment. |
| **TrapLink** | Follow the seed's enabled trap types by default, with an optional custom checklist. Translate only supported recognized effects. Use one-at-a-time processing with a bounded queue by default; discard-extra and compatible-overlap modes optional. Exact limits/gaps remain to be validated. |

Ordinary trap items placed in a seed are separate from TrapLink traffic and must not be dropped by linked-trap overflow preferences. Resource/link events must not rebroadcast themselves or count one pickup twice. New named links are considered only when a meaningful mechanic and compatible peer protocol exist. **GiftLink/gifting is excluded.**

## Goal progress and the finished run

Show progress in the tracker and milestone notifications by default, with an optional persistent counter and the ability to disable milestone popups. Distinguish unlocking Clockwerk from actually defeating him. Combine a page's item/progress message where possible.

After finishing, show a compact summary with **Return to Menu** and **Continue Free Play**. Main information: time to goal, personally completed checks, progression sent, and progression received. An expandable breakdown can include progression found for yourself and known sender/recipient details. Do not assign a numerical usefulness score.

Track active play and total elapsed time across sessions; show both by default, with active play prominent and either display optional. Active play excludes closed-game/genuinely paused time. Loading/cutscene/idle details still need an explicit policy. Freeze completion statistics at the goal; later free play and automatically released checks must not inflate personal contribution totals.

Goal reporting must be durable and distinguish local achievement, server confirmation, and actual release state. Respect the host's release policy. Never simulate release by claiming the player physically collected all remaining locations.

## Cosmetic Sly players

This is visual presence with **independent worlds and saves**, not shared-world co-op. Remote players have no collision and cannot collect your checks, affect enemies, move objects, activate switches, or modify your progression.

| Feature | Planned default and choices |
|---|---|
| Visibility scope | Eligible same-room Sly players, with individual hide controls and a global display toggle. Local sharing is separately optional. |
| Sharing approval | Include in first-session review and remember per profile. Automatic connection is not permission to publish presence. |
| Different areas | Roster/area information available by default; area-change notifications optional. Render a character only in a compatible matching area. |
| Appearance | Translucent normal Sly by default; optional tints, adjustable opacity, and outlines if the renderer supports them cleanly. |
| Names | Nearby labels by default; always-on and roster-only alternatives, plus label sizing. No automatic through-wall disclosure. |
| Overlap | Automatically fade near the local character/camera by default. Fixed-opacity and hide-during-overlap optional. Labels/outlines must not defeat the fading. |
| Crowd limit | Adjustable limit, **three nearby remote ghosts by default**, plus individual hides. Hidden-by-limit players can remain in the roster. Actual maximum needs performance testing. |
| Audio | **Always silent.** No ghost footsteps, jumps, cane swings, voices, or optional action-audio mode. |
| Special sections | **Roster only** for races, vehicles, turrets, and designated minigames. No substitute position markers or cosmetic vehicle replication. |
| Stalled updates | Briefly hold the last position, then fade by default. Immediate hide or a brief disconnected indicator optional. Resume only from fresh compatible data. |
| Death | Immediately disappear—no death/drowning animation, fade, particles, or sound. Reappear at the remote game's confirmed respawn position, usually its checkpoint. |
| Connection | Automatic discovery/connection where feasible, with manual fallback. Relay/NAT/hosting details and any costs remain unresolved. |
| Compatibility | Keep compatible peers visible; hide incompatible data and provide a roster warning with optional guidance. Do not force an update or interrupt otherwise compatible AP gameplay. |

DeathLink is independent of ghost rendering. If both players participate, a death can trigger DeathLink even when the ghost is hidden or in another area. Disappearance itself must never generate another death event. A connection interruption is not evidence of death.

## Setup, updates, and the solo example

- Guided asset setup should offer compatible existing-data reuse and player-owned ISO extraction where supported. Preserve originals and normal saves; validate the actual required extraction format.
- Keep APWorld delivery separate. Version checks by default; an optional explicit install helper shows the destination/existing version and backs up before replacement. No silent overwrite or duplicate conflicting world installation.
- Preserve manual release downloads. Automatic updates between sessions are optional and **off by default**, limited to components the player explicitly manages, with complete backups and a viable rollback path. Save migrations and data re-extraction must not occur invisibly. Current source-first distribution/permission constraints still apply.
- The optional short solo example is deferred to **RC1.5**. It gets an independent profile and a local server that starts only when selected. Resume by default; **Start New Attempt** preserves the previous complete run and uses independent game/receipt/server state.
- An example's placements are generated normally, without plando or a forced route. Once bundled, it is explicitly a fixed example; a new attempt replays those placements, not a newly generated seed. The selected short configuration starts in Tide of Terror, requires one boss then Clockwerk, enables FastClockwerk/cutscene skips, and includes individual bottles, hourglasses, and all supported non-minigame checks. All six minigame categories and minigame caches are excluded. Randomness may require travel; Raleigh is not a forced first boss and a short goal does not guarantee fixed playtime.

## Optional Paris prologue skip

An optional **Skip Paris Prologue** is requested for future work. It would go to episode selection while preserving randomized access—only genuinely unlocked episodes appear. Its default remains unchanged until separately decided.

Before implementing it, resolve the **Paris Files check and reward**: whether a compatible explicit auto-check or an additive world/logic option is appropriate, how the item/location balance is preserved, and how duplicate grants and intro/save flags are prevented. Do not silently remove the check, grant all episodes, or force a routed seed. This is separate from the already implemented earned-hub fast travel.

## Help and later platforms

Accessibility settings should be available before onboarding. Reusable Help and optional tips are planned; a short skippable Sly-specific walkthrough is **tentative**, not a mandatory tutorial or generic explanation of AP.

Windows remains first. Linux follows core reliability; Steam Deck needs handheld/input/readability/Game Mode checks within that effort. macOS depends on suitable upstream support. Practice mode and an installer remain deferred rather than automatically included in RC1.5. None of these items promises an upstream ProjectCane delivery schedule.
