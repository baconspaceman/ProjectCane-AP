# Third-party components and licensing

The root MIT license covers original ProjectCane AP integration code. It does not
relicense the upstream game, game data, or separately licensed dependencies.

## Native game foundation: ProjectCane

The native Sly 1 port is the work of **theclub654 and the ProjectCane contributors**.
Their engine reconstruction and tools make this integration possible:

- [ProjectCane repository](https://github.com/theclub654/ProjectCane)
- [Pinned upstream release](https://github.com/theclub654/ProjectCane/releases/tag/v0.4.1-beta.1)
- Pinned source: `ebc994c7706be07e1150d5da2c276f2a5789c27f`.

At RC1 inspection, the pinned repository did not declare a project-level license
for its own native code. Accordingly, this distribution supplies our integration
additions and a patch/build recipe; it does not ship a ProjectCane executable, full
native upstream source tree, extractor binary, or its FFmpeg DLLs. Users obtain the
pinned upstream from its own repository during local setup. Upstream notices and
third-party terms remain applicable. No ProjectCane endorsement is implied.

## Components included in source or the companion/runtime package

| Component | Version/basis | Notice/source |
|---|---|---|
| Archipelago core | 0.6.8 / `54803be064fc7e80c4628777ed0b46a9390f255f` | MIT root notice in the runtime and `LICENSES/Archipelago.txt`; individual subdirectory exceptions remain applicable. |
| Sly 1 APWorld | hoppel16 `v0.3.5-alpha-hotfix1` / `df6333f582fffb18ed7cbac0ef3e4de7f03c4367`, with additive native metadata/options | Original MIT contributor notice in `world/sly1/LICENSE` and the separate APWorld archive. |
| Sly 1 PopTracker pack | Slinq17 `0288a04071f2db582f1a00a44066566dbf4f8730` | MIT notice retained in `tracker/reference/LICENSE` and `LICENSES/Sly-PopTracker.txt`. |
| Universal Archipelago SDK | Exact exported 0.1.0 source | MIT in `sdk/LICENSE` and `LICENSES/Universal-AP-SDK.txt`. |
| Peliarch / archipelago_rs | Exact exported SDK dependency source and Cargo.lock | Natalie Weizenbaum's MIT notice in `sdk/vendor/peliarch/MIT-LICENSE` and `LICENSES/Peliarch.txt`. |
| CPython standalone | 3.12.13 | Python license and historical notices accompany the portable runtime. Standalone dependency notices are included under `LICENSES`. |
| Python packages | Exact installed versions in packaged metadata | Wheel `*.dist-info` licenses are retained in the runtime; additional notices are under `LICENSES`. |
| orjson | 3.11.7 | MPL-2.0 AND (Apache-2.0 OR MIT), as declared by its package. Matching covered source is included in `SOURCES` with its licenses. |
| Rust dependencies | Exact graph in Cargo.lock | Package-specific notices and a dependency inventory accompany the compiled companion under `LICENSES`. This includes non-MIT components. |

**4laric** is credited for SDK/native-bridge assistance and the Sunshine integration
reference approach. That contribution credit is separate from the legal notices of
the original SDK and dependency authors.

## Locally fetched native dependencies

The ProjectCane checkout includes or fetches GLFW, SDL2, Dear ImGui, GLM, glad,
nlohmann/json, stb, FFmpeg and other dependencies. Their original notices remain in
that checkout. The pinned FFmpeg binary build identifies itself as
`N-126523-g884590dd4a-20260912`, LGPL version 3 or later. It is obtained locally by the
native build workflow rather than redistributed in this companion ZIP. See
[FFmpeg's distribution guidance](https://ffmpeg.org/legal.html) before separately
redistributing a locally assembled native build and its dependencies.

Original Sly game data is not included. The Sly Cooper property belongs to its
respective rights holders. This is an independent experimental fan integration.
