# ProjectCane AP

**Sly Cooper and the Thievius Raccoonus, running natively on Windows with Archipelago.**

**RC1 · Experimental · Windows x64 · One-time local native build**

[Download RC1](https://github.com/baconspaceman/ProjectCane-AP/releases/tag/rc1) ·
[Setup tutorial](docs/GETTING_STARTED.md) · [Known issues](docs/KNOWN_ISSUES.md) ·
[Roadmap](ROADMAP.md)

ProjectCane AP connects a custom build of **ProjectCane** to Archipelago. It preserves the Sly 1 APWorld's checks, items, goals, and logic while replacing the emulator/PINE connection with a native game adapter and companion client.

**ProjectCane makes this project possible.** The native port, engine reconstruction, and original extraction tools are the work of **theclub654 and the ProjectCane contributors**. ProjectCane AP adds the Archipelago integration; it does not claim authorship of the native port. Please visit the [official ProjectCane repository](https://github.com/theclub654/ProjectCane) and [ProjectCane releases](https://github.com/theclub654/ProjectCane/releases) for their work. This independent adaptation is not an official endorsement or release from that team.

**RC1 is the first public, experimental source-first release.** It includes the current native AP integration and in-game tracker, but **you build the modified native game locally**. Multiplayer ghosts, AP links, and the remaining advanced features are planned for **RC1.5**. RC2 will follow further ProjectCane development. See the [roadmap](ROADMAP.md) for the current release boundaries.

This is a custom native game build, not a plugin for an unmodified ProjectCane installation. It is an independent community project; it is not an official Sly Cooper release or an official ProjectCane/Archipelago distribution.

## Play RC1

Download the **Windows companion/runtime ZIP** and, if you are generating a seed, obtain the separately listed compatible **`sly1.apworld`**. Extract the ZIP into its own writable folder.

- [Windows companion/runtime ZIP](https://github.com/baconspaceman/ProjectCane-AP/releases/download/rc1/ProjectCane-AP-RC1-Windows-Source-Assisted.zip)
- [Sly APWorld for the generator](https://github.com/baconspaceman/ProjectCane-AP/releases/download/rc1/sly1.apworld)
- [SHA-256 checksums](https://github.com/baconspaceman/ProjectCane-AP/releases/download/rc1/SHA256SUMS.txt)

**First setup requires Git, CMake, and Visual Studio 2022 Build Tools with Desktop development with C++ and a Windows SDK.** Run `Build-Native.cmd` to fetch the pinned ProjectCane source, apply the AP changes, and build it locally. The downloadable package does **not** contain a ProjectCane game binary, extraction-tool binary, or FFmpeg binary.

You supply compatible extracted ProjectCane data, prepared from your **US Sly 1 PS2 ISO, SCUS-97198**, using ProjectCane's official extraction workflow. Existing compatible data can be reused. Original game files are not included.

Start with the [setup guide](docs/GETTING_STARTED.md). It covers the local build, preparing game data, joining a room, selecting your AP save, and opening the tracker. Initial setup is heavier than a download-and-play binary release. You do not need AP Port Studio or an emulator.

## What is in this version?

- Native item delivery, physical checks, boss/page goal access, and goal reporting.
- In-game server/slot/password fields with Connect, Reconnect, and Disconnect.
- Separate AP save profiles with paired item-delivery records and backups.
- Received/sent item notifications and a recent client-activity list; no room-chat interface.
- Quick and full trackers, inventory quantities, and an in-game map using Slinq17's Sly pack artwork.
- Actual AP world-rule evaluation for check availability and **BK'd** status.
- High contrast, text/panel sizing, an alternate color palette, adjustable popup placement/timing, and a persistent BK notice option.
- An optional FPS/frame-time display and frame-time graph.
- Direct travel to an episode's hub once its episode unlock and at least one matching key have been received. Opening stages remain available for their checks.

The map shows **level markers and check counts**, not the exact position of every bottle. Missing-requirement explanations use an honest fallback where a precise explanation is not implemented. RC1 does not contain every feature discussed during development.

## Important RC1 limitations

**After completing the goal, you may not receive a clear completion popup.** The tested game and server correctly recorded completion, but the dedicated completion notification is missing. A large automatic release/collection batch can also temporarily delay tracker and companion status updates. Earlier notifications can fall out of the recent-event list during that burst.

Do not reset a run or replay the final fight just because the popup is absent. Preserve the profile and consult [known issues and troubleshooting](docs/KNOWN_ISSUES.md).

AP links—including DeathLink—and cosmetic player ghosts are **not available in RC1**. Their YAML preferences are not proof of implemented client support. See [compatibility](docs/COMPATIBILITY.md) before choosing versions or options.

## How far has it been tested?

The native game has been played through physical checks, item deliveries, save/reload, boss access, and the final goal. The recorded ending test was **cheat-assisted with six extra keys** to reach the bosses sooner; the player physically defeated Raleigh and Clockwerk. This verifies the ending/reporting path, not a fully natural randomized playthrough or every option combination.

Automated coverage includes actual Python world rules, native transaction methods, trap timing, and a real local Archipelago server fixture. These provide different kinds of evidence; none replaces multiplayer and gameplay testing. [Read the testing record.](docs/TESTING.md)

## Help and development

- [Setup and joining a room](docs/GETTING_STARTED.md)
- [Controls, tracker, and maps](docs/CONTROLS.md)
- [Compatible components](docs/COMPATIBILITY.md)
- [Known issues](docs/KNOWN_ISSUES.md)
- [Save backups and recovery](docs/SAVES_AND_RECOVERY.md)
- [Testing and reporting problems](docs/TESTING.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)

When reporting a problem, include your release version, what you were doing, and whether the issue survives a normal restart of the same profile. Use the in-game diagnostic export when useful. Do not attach passwords, ISOs, extracted assets, or an unreviewed full profile directory.

## Credits

- [ProjectCane](https://github.com/theclub654/ProjectCane) for the native Sly 1 port.
- [Archipelago](https://github.com/ArchipelagoMW/Archipelago) for the multiworld framework.
- [hoppel16's Sly 1 APWorld](https://github.com/hoppel16/ArchipelagoBranchSly1) for the original world, item/check definitions, and logic.
- [Slinq17's Sly 1 PopTracker pack](https://github.com/Slinq17/Sly1-Poptracker) for tracker/map assets and the external tracker reference.
- **4laric**, for help with the Archipelago client SDK and native-game bridge, and the [native Sunshine AP integration](https://peliarch.ca/er/native-sunshine.html) that informed this project's companion/native-adapter approach.
- The Universal Archipelago SDK for reusable networking and delivery infrastructure.
- Yammel for YAML authoring.

See [credits](docs/CREDITS.md) for the contributors and upstream projects. Component licenses and redistribution permissions apply separately; credit is not a substitute for permission. See the license/third-party materials supplied with the repository and actual release assets.
