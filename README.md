# auto-splitting-test-fixtures

Real engine binaries for auto splitting runtimes to test against, one GitHub release per engine version.

Binaries this size don't belong in git history, so each engine version is a release whose assets hold just the files the tests read.

## Layout

One release per engine version, tagged `<engine>-<version>`. Each asset is one variant of that version: a platform, plus whatever else that engine varies by.

```
unity-6000.5.8f1-win-x64-mono.zip
unity-6000.5.8f1-win-x64-il2cpp.zip
unity-6000.5.8f1-win-x86-mono.zip
unity-6000.5.8f1-win-x86-il2cpp.zip
```

The program inside is the emptiest one the engine builds, made the way a shipped one is. An asset holds everything about that one build worth keeping: the engine's own runtime and metadata files, their symbols where the build produces any, and the log of the build itself. Which files those are is the engine's business, and its tool says.

[`manifest.json`](manifest.json) maps (engine, version, variant) to the asset URL, its size, its sha256, and the archive's files, each with a sha256 of its own. The archive hash checks the download; the per-file hashes check contents, and they make two builds comparable file by file, which the archive hash never is (the log inside carries timestamps). Consumers fetch through it and cache locally.

Engines separate by data, not structure: the tag prefix, the asset names, and the manifest's `engine` field. Tooling lives per engine under `tools/`.

## Tools

- Unity
  - [building fixtures](tools/unity/build)
  - [measuring offsets](tools/unity/measure)

## Licensing

- Source: CC0
- Release assets carry whatever terms the engine ships under
