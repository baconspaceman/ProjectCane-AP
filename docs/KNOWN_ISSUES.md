# Known issues and troubleshooting

These are current RC1 limitations. A release label does not mean they are resolved.

## Goal completed, but no clear completion notification

The dedicated goal-completion notification/summary is missing. In the recorded ending test, the native game, tracker, and AP server all confirmed the goal even though the player did not see an explicit completion message.

**Do not reset the profile or repeat the final fight solely because the popup is absent.** Check the AP room's completion status or ask its host to confirm. Preserve your save if investigating a discrepancy.

The room's release policy controls what happens to remaining checks. Automatic release was enabled in the development test; another host may choose a different policy. Released checks are not evidence that you physically collected every location.

## Tracker/status briefly goes stale during a large item burst

When a goal triggers a large release/collection batch, current delivery processing can delay companion UI/status publication. The tracker may temporarily report stale/unknown data, and notifications can arrive late. The recent-event ring can also drop earlier messages during a sufficiently large burst.

Allow processing to settle and inspect F8's received/delivered/pending information. Do not delete the item journal or save to try to force a refresh. If pending deliveries do not settle, preserve the profile and report it. Fixing this requires bounded delivery work and durable goal/UI state; it is not considered fixed merely because a small packet works.

## Server connected, game not ready

Connection and gameplay readiness are separate. Close F8 and select **Start AP profile** or **Resume AP profile** in the first save slot. Item effects wait for a compatible loaded game state.

If the adapter reports an identity/save mismatch, stop and check that the profile belongs to this seed and slot. Do not edit identity tokens or swap isolated save files to bypass the check.

## Empty or unexpected travel map

The current build includes fixes for selecting a newly unlocked, unvisited episode and direct hub travel once an episode unlock plus a matching key are received. Older development binaries may lack those fixes. Use the matching release components.

A locked hub can be correct: a stage's physical key check can send a bottle or another reward, while the episode key you need is somewhere else. Consult the inventory and world-wide tracker before concluding that a collection failed.

## Movement or frame-time problems

The native adapter's disposable status writes were moved off the game thread after a reported movement hitch. The owner subsequently reported improved performance. This is not proof that every input or rendering problem has the same cause.

Use F8's FPS/frame-time display and optional graph. Report whether a spike accompanies the issue, your controller/input method, the current area, and whether AP panels were open. Avoid changing several control/frame settings at once when comparing behavior.

## Known feature gaps

- AP links and cosmetic ghosts are unavailable in RC1.
- The dedicated completion summary, full per-event notification configuration, optional notification sound cues, complete profile management, and automatic updater are not yet included.
- Tracker shortcuts are not yet rebindable and do not yet have controller combinations.
- Detailed missing-requirement text uses a fallback for rules without an implemented explanation.
- In-game maps show level summaries, not individual bottle-position navigation.
- External PopTracker acceptance is pending; the in-game map does not establish external-pack compatibility.
- A prologue/Paris skip is future work. Paris Files remains an actual check in the current world.

## Verification gaps

Original trap hooks and transaction/timer tests exist, but the short development seed used no traps. Physical behavior across all four traps, option combinations, minigames, and speedrun tricks needs more coverage. The ending test used extra keys; a natural randomized start-to-goal run is not established by that result.

## Reporting a problem

Open **F8 → Connect → Troubleshooting → Export diagnostic report** for a local, limited report. It does not upload automatically. Include the release version and a concise reproduction description.

Review any other logs before sharing them. Do not attach passwords, an entire unreviewed profile, original ISO files, or extracted assets. Keep a private complete backup of the affected profile so recovery remains possible.
