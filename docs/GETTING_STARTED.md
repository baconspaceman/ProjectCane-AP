# Getting started with RC1 on Windows

RC1 is a **source-first experimental release**. The companion/runtime is packaged, but the modified ProjectCane game is built on your computer. Allow time for that first build and game-data preparation.

The native engine and extraction tools come from **theclub654 and the ProjectCane contributors**. This integration depends on their work. Use their [official repository](https://github.com/theclub654/ProjectCane) and [release page](https://github.com/theclub654/ProjectCane/releases) for upstream instructions and extraction tools; ProjectCane AP adds the AP adapter rather than replacing their authorship.

## 1. Prepare the prerequisites

You need:

- Windows and a writable folder for this release.
- **Git** and **CMake** available to the build script.
- **Visual Studio 2022 Build Tools**, with **Desktop development with C++** and a Windows SDK installed.
- Compatible Sly 1 game data, described below.
- An AP room address and the exact Sly slot name for the run you want to join.

The release bundles the companion and portable Python/AP logic runtime. You do not need a separate Python installation for the packaged player workflow. You do not need AP Port Studio or PCSX2/PINE.

The public ZIP intentionally does **not** include `ProjectCane.exe`, extraction-tool binaries, FFmpeg binaries, an ISO, or extracted game assets. The native build step supplies the custom game from its pinned upstream source. If you use ProjectCane's official GUI extractor, follow its own requirements; its .NET 9 requirement is separate from running the AP companion.

## 2. Extract the release and build the native game

Extract the companion/runtime ZIP to a new folder. Keep it separate from your ordinary ProjectCane installation. Do not run it directly inside the ZIP viewer.

Run:

```powershell
.\Build-Native.cmd
```

The wrapper fetches the pinned ProjectCane source, applies the integration patches/additions, builds locally, and stages the resulting game and needed native dependencies into the package. It uses the release's selected version; it does not mean that the newest stock ProjectCane executable can replace it.

If the build reports a missing compiler or SDK, resolve that prerequisite and rerun the documented build step. Do not copy a stock game executable into place to bypass an AP build error.

## 3. Prepare compatible game data

Use your own **US Sly Cooper and the Thievius Raccoonus PS2 ISO, SCUS-97198**, with the **official ProjectCane v0.4.1 extraction workflow**. You can reuse existing data only if it is compatible with this build.

This baseline expects the newer **PCBR17** level data and the corresponding extracted PCM/media data. Older extracted levels or compressed-audio files from another version are not automatically interchangeable. Follow the official extractor's instructions for the full level and media extraction; selecting the ISO itself as an already-extracted data folder will not work.

Then register your extracted folder with:

```powershell
.\Setup.cmd --data "D:\Games\SlyData"
```

Replace the example path with your actual extracted-data directory. Setup validates the supplied data configuration. It does not download an ISO or supply missing original game assets. AP saves remain separate from that data folder.

## 4. Prepare the AP room

If somebody else hosts/generates the room, give them your Sly YAML and use the server address and slot name they provide.

If you generate the seed yourself, use the compatible separately supplied/listed `sly1.apworld` in that generator's `custom_worlds` folder. Back up an existing Sly world before replacing it, and avoid two conflicting Sly packages. See [compatibility](COMPATIBILITY.md).

### Host: generate a fresh seed with the example YAML

The repository includes [examples/SlyPlayer.yaml](../examples/SlyPlayer.yaml), authored through **Yammel** and validated against the native Sly world's option definitions. It is a **configuration example, not a fixed seed or a played save**.

1. Use your own **Archipelago 0.6.8 generator installation**. Install the compatible native `sly1.apworld` in that installation's `custom_worlds` folder, keeping a backup of any previous Sly package.
2. Place a copy of `SlyPlayer.yaml` in that installation's **`Players`** folder. Use the generator's configured player-input folder instead if you have deliberately changed it. Do not put the YAML inside the APWorld archive or this package's internal Python runtime.
3. Change the player name if desired. For multiple players, give every YAML a distinct player name and place each intended participant's YAML in the same input folder. Keep unrelated old YAMLs outside the selected input folder so they are not accidentally included.
4. Run the installation's normal **Generate** entry through the Archipelago Launcher. Use normal random seed generation; the example does not supply a seed number or plando placements.
5. Host the generated output using your normal Archipelago server/room workflow. Generation alone does not start a room. Keep the resulting output for that run rather than regenerating it when resuming.
6. Connect ProjectCane AP to that room using the exact generated slot name—**`SlyPlayer`** if you did not rename it—and the room's address/port.

The example starts with Tide of Terror available, requires **one boss victory** to unlock Clockwerk, enables **FastClockwerk** and cutscene skipping, and includes individual bottle checks/items plus hourglasses. It excludes all six minigame-check categories and adds no minigame caches. **TrapChance is explicitly zero**, and DeathLink, RingLink, EnergyLink, DamageLink, and TrapLink are explicitly false because RC1 does not implement those links.

All eligible item placements remain randomized. Starting in Tide does not force Raleigh to be your first reachable boss, and a low goal threshold does not guarantee a particular playtime. You can finish the goal without clearing every enabled check.

RC1's public workflow joins an existing room. The optional bundled solo example is deferred to RC1.5. No developer's played seed/profile is shipped as your new game.

## 5. Create a named run and launch

From the package folder, run:

```powershell
.\Play.cmd --profile "MySlyRun" --server "archipelago.gg:12345" --slot "YourExactSlotName"
```

Replace the address/port and slot name with your room's values. Choose a new profile name for a new seed. Do not put a room password into a command line; enter it in the game's password field if required.

In the game:

1. Open **F8** and check the server and slot fields.
2. Enter the password if the room needs one, then press **Connect**.
3. Close F8 and select **Start AP profile** or **Resume AP profile** in the first save slot.
4. After the game loads, F8 should distinguish **Server: Connected** from **Game: Ready**.

The first save slot represents this configured AP profile. Other vanilla slots are intentionally unavailable for that run. The regular ProjectCane save is not used as a fallback.

## 6. Open the tracker

- Hold **F9** for quick view.
- Press **F10** for the full tracker.
- Press **F11** to switch the full tracker between view mode and interaction mode.
- Enable **Map** for level markers and check summaries.

The game keeps running while these panels are open. See [controls and tracker help](CONTROLS.md) before typing in a dangerous area.

## 7. Resume safely

Save and close the game normally. Resume the named profile with:

```powershell
.\Play.cmd --profile "MySlyRun"
```

The profile retains its server and slot configuration; connect from the in-game menu. Keep that profile paired with its original seed/slot and matching game/client version.

Use a new profile for a new run; do not rename another seed's save or edit identity tokens. Before updates or recovery work, back up the whole profile as described in [save recovery](SAVES_AND_RECOVERY.md).

Read the [known issues](KNOWN_ISSUES.md), especially the missing completion notification and possible delayed status during a large automatic release batch. A missing popup does not necessarily mean the goal was lost.
