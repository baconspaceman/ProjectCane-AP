# Testing and evidence

The following describes development evidence recorded on **October 15, 2026**. It is deliberately narrower than a claim that every option, device, or multiworld works.

## Physical owner play

The owner played the native game through Paris and further platforming, producing actual checks and received items. Same-profile save/close/reopen trials preserved checks, inventory, and receipts. The build also exercised the native tracker, inventory display, and map rendering.

For the ending test, the owner requested assistance to avoid a long key grind. Six missing Tide of Terror keys were supplied through the local AP server's item mechanism; the SDK/native delivery path applied them. This did not fabricate boss checks or send a fake goal packet. The owner then physically defeated Raleigh and Clockwerk.

The settled completed-run evidence recorded:

- Native goal state true.
- Tracker goal state true.
- AP server goal status **30**.
- **658** checks released under that room's automatic release policy.
- **665** completed delivery records and zero pending, including the initial item and six supplied keys.

**This was a cheat-assisted ending test.** It verifies that physical boss/goal events reached the server and that the resulting delivery batch eventually settled. It is not a natural randomized campaign, a record of 658 physically collected checks, or proof of cross-game multiplayer/ghost behavior.

The same test exposed the missing explicit completion notification and delayed status during bulk delivery. Those are documented as [known issues](KNOWN_ISSUES.md), not omitted from the result.

## Automated engineering checks

Recorded checks cover different layers:

| Layer | Evidence | What it does not prove |
|---|---|---|
| Python world/logic | 22 rule/mapping/option tests, using actual Sly rules and immutable upstream data. | Physical collection triggers, collision, UI rendering, or all gameplay routes. |
| Native transaction methods | 52 compiled assertions through the recorded status-thread build, including receipt identity/replay, save failure handling, travel rules, and bounded status publishing. | A complete live-game playthrough or every crash boundary. |
| Trap runtime | Standalone timing/capability/retry assertions and native method integration checks. | Physical feel/visual correctness of every trap in every scene. |
| Companion | Unit/process checks and a real local AP server fixture. | Internet conditions, all other games, or every room configuration. |
| Windows bridge I/O | Concurrent read/write regression with coherent snapshots and bounded transient retries. | All storage devices, antivirus interactions, or failure modes. |
| Yammel | 19 CLI parser and 9 schema-export checks supporting the authored YAML workflow. | Full randomized gameplay acceptance. |

Source/build tests and live game observations are complementary. A compiled executable alone is not gameplay evidence.

## Useful player test reports

Start with a short randomized run and focus on one behavior at a time:

1. Connect, select the correct AP profile, and verify both server and game readiness.
2. Collect a real bottle/key/vault check and compare the notification, inventory, and tracker.
3. Receive progression for another area and confirm the appropriate access changes.
4. Save, close normally, reopen, and confirm progress without extra rewards.
5. Test BK status against the full world, not only your current map.
6. Report the actual final boss/goal result separately from the completion popup.

Randomized placements remain randomized. A low goal threshold reduces the requirement but does not guarantee a fixed playtime. Do not submit an assisted run as an unassisted completion.

Cross-game sessions and two-Sly cosmetic ghost sessions belong to subsequent coverage as their supported features become available. Do not enable or claim a future link simply because its preference appears in a YAML.
