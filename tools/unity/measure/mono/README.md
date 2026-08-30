# Measuring Mono offsets

Finds where Mono keeps the members of its own structures, in the exact library a game ships.

The library is loaded and booted headless through Mono's own embedding exports. Each member is then found by scanning the live structures for values the API hands back. A member is only reported once several independent witnesses agree on where it sits, and anything ambiguous is reported as ambiguous rather than guessed at.

No game is run and nothing is attached to. The library is loaded into the measuring process itself.

## Usage

From `tools/unity/measure/mono`:

```
cargo run --release -- -e "/path/to/Unity/Hub/Editor/6000.5.8f1/Editor/Unity.exe" -l /path/to/player/libmonobdwgc-2.0.so -o measured.json
```

| | | | |
|---|---|---|---|
| `-e` | `--editor` | **required** | Path to the editor binary |
| `-l` | `--library` | **required** | Path to the Mono library out of a built player |
| `-o` | `--out` | **required** | File the measurements are written to |

The editor binary is the same one the builder takes. Mono needs a class library and its config to boot, and both come out of that editor install, so the version has to be the one the player was built with.

It prints how many members it settled and where it wrote them. The measurements themselves go to a file rather than to stdout because Mono prints there itself, and because it sometimes aborts while shutting down, after the file is already written.

## Both architectures on Apple Silicon

Unity's Mac player is a universal binary, and a library loads the slice matching the process that loads it, so one machine can measure both.

```
cargo run --release -- -e ... -l ... -o arm64.json
cargo run --release --target x86_64-apple-darwin -- -e ... -l ... -o x86_64.json
```

The second one runs under Rosetta. Run `rustup target add x86_64-apple-darwin` first, since only the host's own target is installed by default. Whether the two agree decides whether one set of offsets can serve every Mac.
