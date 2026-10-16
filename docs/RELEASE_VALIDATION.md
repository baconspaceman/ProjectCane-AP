# RC1 package validation

These release checks are distinct from the human gameplay record in [TESTING.md](TESTING.md).

- The exported native patch and added files were applied to a fresh checkout at
  the exact pinned ProjectCane commit, in a separate folder containing spaces.
- The published Windows PowerShell build script completed a single-job Release
  build and staged the native executable/dependency files locally.
- That cleanly built executable passed all **52 native method assertions** using
  synthetic game state. This is not a second physical campaign.
- The exported companion source passed its **10 unit tests** with locked dependencies.
- The standalone Python runtime was moved into a different folder with spaces.
  Its executable, base prefix and imports resolved inside that package, without a
  developer venv or global Python installation.
- The player setup validator accepted matching PCBR17/PCM data and rejected older
  incompatible media. These checks read local assets; they did not bundle them.
- A generic external-room profile could be created and reopened without starting
  a game, server or network connection during that preparation check.
- The separate APWorld passed AP0.6.8 static archive/manifest validation with zero
  errors or warnings.

RC1 is not certified across every Windows configuration, GPU/controller, storage
device, internet connection, option combination, or multiworld. There are no game
assets or played profiles in the public archive. See [compatibility](COMPATIBILITY.md)
and [known issues](KNOWN_ISSUES.md) before testing.
