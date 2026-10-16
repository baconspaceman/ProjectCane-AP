# Controls, tracker, and maps

## AP interface shortcuts

| Key | Action |
|---|---|
| **F8** | Open/close connection, accessibility/display, recent activity, and help. |
| **Hold F9** | Show quick tracker view; release to hide it. |
| **F10** | Open/close the full tracker. |
| **F11** | While the full tracker is open, switch between controlling Sly and interacting with the tracker. |

The full tracker opens in view mode, so you can keep moving. **The world keeps running while you interact with the AP interface.** Find a safe position before typing or reading a map.

AP tracker shortcuts are currently keyboard-based. Rebinding these shortcuts and controller combinations belong to later work. Native game movement/controller bindings remain available through ProjectCane's controls.

After the title screen, select **Start AP profile** or **Resume AP profile** in the first save slot. The other slots are intentionally unavailable for that configured run. The in-game Help tab describes the default confirmation key/controller button; use your configured binding if it differs.

## Check list

The tracker labels checks as available, unavailable, completed, or unknown. Availability comes from the Sly APWorld's actual rules and the current session's options/items. Physically obtaining an out-of-logic check remains possible; the tracker is a logic guide, not an input restriction.

Some blocked checks display:

> Unavailable in current logic; detailed requirements not available.

That is an explicit explanation limitation. It is preferable to inventing a required move or key count.

## BK status

**BK'd** means that there are no unchecked locations currently available in the world's logic. It evaluates your whole enabled world, not only the area selected on the map.

**Unknown** means current information is unavailable, incomplete, or stale. It does not mean you are BK'd. A connection problem or a delayed logic update must not be interpreted as a confirmed block.

The optional **Persistent BK notice** setting keeps a notice onscreen while blocked. A filler item does not necessarily unblock anything.

## Maps and inventory

Enable **Map** in the full tracker to display Slinq17's level map. Markers summarize a level:

- A number: available checks.
- `!`: blocked checks.
- `x`: completed.
- `>`: an episode/map link.

Hover for counts and click a marker to open that level's check list. These are **level markers**, not precise bottle coordinates.

The inventory section reports received quantities and delivery status. **Delivered** means the native adapter acknowledged an item; it is not a live count of your unspent lives or charms. A pending item may be waiting for a safe game state or require attention.

## Travel and randomness

Once you have an episode unlock and at least one matching episode key, you can use its map to enter the hub directly instead of replaying the opening stage solely to reach it. The opening stage remains selectable for checks.

Collecting a stage key completes its check; its randomized reward may be something completely different or belong to another player. Episode access and key gates follow the items you receive. An unopened gate does not, by itself, mean a check failed.

## Accessibility and notifications

F8's Accessibility & display tab currently offers high contrast, reduced-animation preference, separate text/panel sizing, Standard or Blue/orange colors, compact/detailed messages, durations, six anchored positions, floating placement, and the BK notice. Use **Save presentation settings** to retain changes.

Notifications are currently silent. Their category labels accompany colors; color alone is not the only indication. The Recent activity tab contains client events, not AP room chat. The broader per-event controls and optional sound cues are future features.
