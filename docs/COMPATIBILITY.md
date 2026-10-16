# Compatibility

RC1 targets **Windows** and a matching set of game, client, world, and extracted data. A successful launch with one component does not certify arbitrary version mixtures.

| Component | RC1 baseline |
|---|---|
| Native engine | Custom ProjectCane build based on **v0.4.1-beta.1**, commit `ebc994c7706be07e1150d5da2c276f2a5789c27f`. |
| Archipelago core | **0.6.8**, commit `54803be064fc7e80c4628777ed0b46a9390f255f`. |
| Original Sly APWorld | **v0.3.5-alpha-hotfix1**, extended additively for native metadata/preferences. |
| AP identity | **Sly Cooper and the Thievius Raccoonus**. Original item/check IDs and world rules are preserved. |
| Native bridge | `sly1-native-v1`, version 1. |
| Map reference | Slinq17/Sly1-Poptracker **v0.1.1-beta**, source commit `0288a04071f2db582f1a00a44066566dbf4f8730`. The pack's internal manifest still identifies itself as `0.1.0-beta`. |
| Original game input | Compatible **US PS2 ISO, SCUS-97198**. No original game data is bundled. |

Use the custom game built from the release's pinned source/patches with that release's companion. The public package omits the native game binary; `Build-Native.cmd` builds it locally. **Do not replace that custom native executable with a stock ProjectCane update.** That removes or changes the AP hooks. ProjectCane v0.4.1 also changed extraction requirements; an older extracted-data directory is not automatically compatible.

## The APWorld download

The separate `sly1.apworld` is for the Archipelago installation used to generate the seed. Keep one appropriate Sly world installed under this game identity. Back up an existing package before replacing it; do not install conflicting copies alongside each other.

The native extension preserves the original 20 options and adds bridge metadata plus link-review preference fields. **A YAML option requesting a link does not mean RC1 implements that link.** DeathLink, RingLink, EnergyLink, DamageLink, and TrapLink are not available in this release. GiftLink is not planned.

The current tested solo seed used the original world contract. Seeds with every other option combination have not completed physical acceptance. Report unsupported behavior with the exact options/version rather than assuming the package name alone guarantees compatibility.

## External PopTracker

The **in-game map/track list** is implemented and uses the pinned pack as its map reference. The separate PopTracker application is not required to use it.

External Sly PopTracker support has not completed compatibility acceptance for this native release. The reference pack has known flat-slot-data and Key Cache mapping differences from the selected world. Do not describe a separately downloaded pack as verified merely because its map images also appear in the game.

## Platforms and upstream evolution

Linux/Steam Deck support is planned later. macOS is conditional on suitable ProjectCane support. No compatibility claim is made for these systems in RC1.

This is a **custom native build**, not an official mod loader/plugin API. Future upstream mod support may provide a different integration path, but its availability and timing are not promised here. Save, asset, or protocol compatibility between RC1, RC1.5, and RC2 will be documented explicitly as those releases exist.
