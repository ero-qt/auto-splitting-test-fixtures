# unity-fixtures

Builds one Unity version's fixture assets: an empty player per variant, then just the files the tests read, zipped and hashed, with the manifest entries printed to paste into `manifest.json`.

> [!IMPORTANT]  
> Mac IL2CPP is missing.  
> It needs a host with the module installed; every other variant is verified.

## Setup

1. Install the editor version through Unity Hub.
2. Add the build support module each variant needs. A Windows host builds its own Mono variants with the editor alone and needs a module for everything else: `Windows Build Support (IL2CPP)`, `Linux Build Support (Mono)`, `Linux Build Support (IL2CPP)`, `Mac Build Support (Mono)`, and `Mac Build Support (IL2CPP)`.
3. Have a Rust toolchain; the tool builds with stable cargo.
4. Keep about 4 GB free per version. A run of every variant on 6000.5.8f1 leaves 3.2 GB in the workspace: 1.9 GB of project, 1.1 GB of players, and 223 MB of assets, the only part worth keeping once a release is up.

The x86 variants need an editor that still ships a 32-bit player; narrow `-v` on versions that dropped it.

## Usage

From `tools/unity`:

```
cargo run --release -- -e "/path/to/Unity/Hub/Editor/6000.5.8f1/Editor/Unity.exe" -o /path/to/destination
```

The editor binary is `Editor/Unity.exe` under a Hub install on Windows, `Editor/Unity` on Linux, and `Unity.app/Contents/MacOS/Unity` on macOS.

| | | | |
|---|---|---|---|
| `-e` | `--editor` | **required** | Path to the editor binary |
| | `--editor-version` | optional | Editor version, when the editor path does not name it |
| `-o` | `--out` | **required** | Workspace to build into |
| `-v` | `--variant` | optional | Build only these variants. Every variant builds without it |

Variants are `win-x64-mono`, `win-x64-il2cpp`, `win-x86-mono`, `win-x86-il2cpp`, `linux-x64-mono`, `linux-x64-il2cpp`, `mac-mono`, and `mac-il2cpp`, passed together or one flag at a time:

```
cargo run --release -- -e "/path/to/Unity.exe" -o /path/to/destination -v win-x64-mono linux-x64-mono
```

The workspace holds one directory per version. Inside it, `project/` is a throwaway empty project reused across runs, each variant builds into its own directory, and the assets are written next to them:

```
<out>/6000.5.8f1/
├── project/
├── win-x64-mono/
├── unity-6000.5.8f1-win-x64-mono.zip
└── unity-6000.5.8f1-win-x64-mono.log
```

Every editor run keeps its log beside what it produced, and a build's log goes into its asset too. The log opens with the command line it ran, so it records what produced the asset. The run prints how long each build took and ends with the manifest entries.

The editor does the building: `editor/FixtureBuild.cs` is copied into the throwaway project and invoked through `-executeMethod`, taking the output path and scripting backend as arguments.

## What goes in an asset

The binaries, matched by name whatever extension the platform gives them and wherever in the build they sit:

- `UnityPlayer`, always
- mono variants: `mono-2.0-bdwgc`, or older editors' `mono`
- IL2CPP variants: `GameAssembly` and `global-metadata.dat`

Only from the paths a shipped game has. Builds keep a `BackUpThisFolder_ButDontShipItWithYourGame` directory beside the player holding a second copy of the metadata, among other things a game does not ship.

A variant that finds fewer binaries than its backend expects fails instead of shipping an incomplete asset.

Then their symbols, which come out of that backup directory: `GameAssembly.pdb` on Windows, `GameAssembly.debug` and `UnityPlayer_s.debug` on Linux. IL2CPP compiles the runtime's own structures into the game assembly, so those symbols name the layouts a walk over that build has to know. They roughly double an IL2CPP asset, which beats keeping them somewhere a build can drift away from.

Mono builds carry none. The editor keeps a `mono-2.0-bdwgc.pdb` at its root and `UnityPlayer` symbols under its player variations, neither attached to the copy a player ships, so pairing them is a question of matching debug IDs rather than paths.

Last, the editor's log for that build, as `build.log`.

## Cutting a release

Releases are cut by hand, since building needs installed editors:

1. Build the variants for the version.
2. Create the release tagged `unity-<version>` and upload the zips.
3. PR the printed manifest entries into `manifest.json`.
