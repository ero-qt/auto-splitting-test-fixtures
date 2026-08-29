//! Builds one Unity version's fixture assets: an empty player per variant,
//! then just the files the tests read, zipped and hashed, with the manifest
//! entries printed to paste into manifest.json.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{self, Command},
    time::{Duration, Instant},
};

use clap::Parser;
use sha2::{Digest, Sha256};
use zip::{write::SimpleFileOptions, ZipWriter};

const RELEASES: &str =
    "https://github.com/LiveSplit/auto-splitting-test-fixtures/releases/download";

/// Variant name, the editor's build target, and the scripting backend.
const VARIANTS: &[(&str, &str, &str)] = &[
    ("win-x64-mono", "StandaloneWindows64", "mono"),
    ("win-x64-il2cpp", "StandaloneWindows64", "il2cpp"),
    ("win-x86-mono", "StandaloneWindows", "mono"),
    ("win-x86-il2cpp", "StandaloneWindows", "il2cpp"),
    ("linux-x64-mono", "StandaloneLinux64", "mono"),
    ("linux-x64-il2cpp", "StandaloneLinux64", "il2cpp"),
    ("mac-mono", "StandaloneOSX", "mono"),
    ("mac-il2cpp", "StandaloneOSX", "il2cpp"),
];

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(1);
}

/// The name with the extension its platform gave it taken off.
fn stem(name: &str) -> &str {
    name.strip_suffix(".dll")
        .or_else(|| name.strip_suffix(".so"))
        .or_else(|| name.strip_suffix(".dylib"))
        .or_else(|| name.strip_suffix(".exe"))
        .unwrap_or(name)
}

/// Whether a built file is one the tests read: the player binary always,
/// the runtime library on mono, the game assembly and the metadata file on
/// IL2CPP. Matched by name, wherever in the build it sits.
fn wanted(backend: &str, name: &str) -> bool {
    wanted_stem(backend, stem(name))
}

/// The names those files carry, with the extension each platform gives
/// them taken off.
fn wanted_stem(backend: &str, stem: &str) -> bool {
    match backend {
        "mono" => matches!(
            stem,
            "UnityPlayer"
                | "mono"
                | "mono-2.0-bdwgc"
                | "libmono"
                | "libmono.0"
                | "libmonobdwgc-2.0"
        ),
        _ => matches!(stem, "UnityPlayer" | "GameAssembly" | "global-metadata.dat"),
    }
}

/// The symbols belonging to a file the tests read, as a PDB on Windows or
/// separated debug info elsewhere. IL2CPP compiles the runtime's own
/// structures into the game assembly, so its symbols name the layouts a
/// walk over that build has to know.
fn wanted_symbols(backend: &str, name: &str) -> bool {
    name.strip_suffix(".pdb")
        .or_else(|| name.strip_suffix(".debug"))
        .map(|stem| stem.strip_suffix("_s").unwrap_or(stem))
        .is_some_and(|stem| wanted_stem(backend, stem))
}

/// The directory a build puts beside the player for the files a game does
/// not ship, symbols among them.
const NOT_SHIPPED: &str = "_BackUpThisFolder_ButDontShipItWithYourGame";

/// Every file under the build, each with the path a shipped game would
/// hold it at.
fn walk(root: &Path, at: &str, into: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(root).unwrap_or_else(|_| fail("build directory unreadable")) {
        let path = entry.expect("directory entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let relative = match at {
            "" => name.to_string(),
            _ => format!("{at}/{name}"),
        };

        if path.is_dir() {
            walk(&path, &relative, into);
        } else {
            into.push((relative, path));
        }
    }
}

/// Zips the files at their in-build relative paths, answering the archive's
/// size and digest, and each file's own digest, so identical builds stay
/// comparable file by file even though the archive's bytes never repeat.
fn archive(files: &[(String, PathBuf)], asset: &Path) -> (u64, String, Vec<serde_json::Value>) {
    let mut zip = ZipWriter::new(fs::File::create(asset).expect("creating the asset"));
    let mut entries = Vec::new();
    for (relative, file) in files {
        let bytes = fs::read(file).expect("reading a built file");
        zip.start_file(relative, SimpleFileOptions::default())
            .expect("starting the zip entry");
        zip.write_all(&bytes).expect("writing the zip entry");
        entries.push(serde_json::json!({
            "path": relative,
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
        }));
    }
    zip.finish().expect("finishing the asset");

    let bytes = fs::read(asset).expect("re-reading the asset");
    (
        bytes.len() as u64,
        format!("{:x}", Sha256::digest(&bytes)),
        entries,
    )
}

/// Runs the editor with its log kept beside whatever it produces. The log
/// carries the build report and the engine's own version lines, which is
/// what says how an asset came to be.
fn run_editor(editor: &Path, log: &Path, args: &[&str]) -> Duration {
    let mut command = Command::new(editor);
    command
        .args(["-batchmode", "-nographics", "-quit"])
        .args(["-logFile", &log.to_string_lossy()])
        .args(args);
    println!("> {command:?}");

    let started = Instant::now();
    match command.status() {
        Ok(status) if status.success() => started.elapsed(),
        Ok(_) => fail(&format!("editor exited nonzero, see {}", log.display())),
        Err(error) => fail(&format!("editor would not start: {error}")),
    }
}

/// The mac build target's command line name, which 2017.3 renamed.
fn mac_target(version: &str) -> &'static str {
    let mut parts = version.split(['.', 'a', 'b', 'f', 'p']);
    let mut next = || {
        parts
            .next()
            .and_then(|part| part.parse().ok())
            .unwrap_or(0u32)
    };
    if (next(), next()) < (2017, 3) {
        "StandaloneOSXUniversal"
    } else {
        "StandaloneOSX"
    }
}

/// The version out of a Hub-style editor path, such as
/// `.../Hub/Editor/6000.5.8f1/Editor/Unity.exe`.
fn version_from(editor: &Path) -> Option<String> {
    editor.components().rev().find_map(|component| {
        let text = component.as_os_str().to_str()?;
        let looks_like_version = text.starts_with(|c: char| c.is_ascii_digit())
            && text.matches('.').count() == 2
            && text.contains(['a', 'b', 'f', 'p']);
        looks_like_version.then(|| text.to_string())
    })
}

/// Builds one Unity version's fixture assets: an empty player per variant,
/// zipped and hashed down to the files the tests read, with the manifest
/// entries printed to paste into manifest.json.
#[derive(Parser)]
struct Args {
    /// Path to the editor binary
    #[arg(short, long)]
    editor: PathBuf,

    /// Workspace the projects and players build into
    #[arg(short, long)]
    out: PathBuf,

    /// Editor version, when the editor path does not name it
    #[arg(long)]
    editor_version: Option<String>,

    /// Narrows the run to these variants. Every variant builds without
    /// it, which needs every module installed
    #[arg(short = 'v', long = "variant", num_args = 1.., value_parser = known_variant)]
    variants: Vec<String>,
}

fn known_variant(value: &str) -> Result<String, String> {
    let known = VARIANTS.iter().any(|(name, ..)| *name == value);
    known.then(|| value.to_string()).ok_or_else(|| {
        let names: Vec<_> = VARIANTS.iter().map(|(name, ..)| *name).collect();
        format!("one of {}", names.join(", "))
    })
}

fn main() {
    let mut args = Args::parse();
    if args.variants.is_empty() {
        args.variants = VARIANTS.iter().map(|(name, ..)| name.to_string()).collect();
    }
    let version = args
        .editor_version
        .or_else(|| version_from(&args.editor))
        .unwrap_or_else(|| fail("editor path names no version, pass --editor-version"));

    let workspace = args.out.join(&version);
    let project = workspace.join("project");
    if !project.exists() {
        fs::create_dir_all(&workspace).expect("creating the workspace");
        run_editor(
            &args.editor,
            &workspace.join("create-project.log"),
            &["-createProject", &project.to_string_lossy()],
        );
    }

    let editor_scripts = project.join("Assets").join("Editor");
    fs::create_dir_all(&editor_scripts).expect("creating Assets/Editor");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("editor/FixtureBuild.cs"),
        editor_scripts.join("FixtureBuild.cs"),
    )
    .expect("copying FixtureBuild.cs");

    let mut manifest = Vec::new();
    for variant in &args.variants {
        let &(name, target, backend) = VARIANTS
            .iter()
            .find(|(name, ..)| name == variant)
            .expect("validated above");
        let target = match target {
            "StandaloneOSX" => mac_target(&version),
            _ => target,
        };

        let build_dir = workspace.join(name);
        let log = workspace.join(format!("unity-{version}-{name}.log"));
        let took = run_editor(
            &args.editor,
            &log,
            &[
                "-projectPath",
                &project.to_string_lossy(),
                "-buildTarget",
                target,
                "-executeMethod",
                "FixtureBuild.Build",
                "-fixtureOut",
                &build_dir.to_string_lossy(),
                "-fixtureBackend",
                backend,
            ],
        );

        let mut built = Vec::new();
        walk(&build_dir, "", &mut built);
        built.sort();

        let last = |relative: &str| relative.rsplit('/').next().unwrap_or(relative).to_string();

        // The binaries at the paths a shipped game holds them at, and the
        // symbols for those binaries, which a build keeps in a directory of
        // its own beside the player.
        let mut binaries: Vec<_> = built
            .iter()
            .filter(|(relative, _)| {
                !relative.contains(NOT_SHIPPED) && wanted(backend, &last(relative))
            })
            .cloned()
            .collect();

        // Players from before the engine split one out are monolithic, on
        // Windows and Mac until 2017 and on Linux until 2019: no UnityPlayer
        // library exists, and the executable itself is the player.
        let names_a_player = |stem: &str| stem == "UnityPlayer" || stem == "fixture";
        if !binaries
            .iter()
            .any(|(relative, _)| names_a_player(stem(&last(relative))))
        {
            binaries.extend(
                built
                    .iter()
                    .find(|(relative, _)| {
                        !relative.contains(NOT_SHIPPED) && stem(&last(relative)) == "fixture"
                    })
                    .cloned(),
            );
        }

        // By role, not by count: a duplicate match for one role must not
        // stand in for another role's absence.
        let holds = |role: &dyn Fn(&str) -> bool| {
            binaries
                .iter()
                .any(|(relative, _)| role(stem(&last(relative))))
        };
        let complete = holds(&names_a_player)
            && match backend {
                "mono" => holds(&|stem| !names_a_player(stem)),
                _ => {
                    holds(&|stem| stem == "GameAssembly")
                        && holds(&|stem| stem == "global-metadata.dat")
                }
            };
        if !complete {
            fail(&format!(
                "{name}: build under {} is missing binaries the asset needs",
                build_dir.display(),
            ));
        }

        let mut files = binaries;
        files.extend(
            built
                .iter()
                .filter(|(relative, _)| wanted_symbols(backend, &last(relative)))
                .cloned(),
        );
        files.push(("build.log".to_string(), log));

        let asset = workspace.join(format!("unity-{version}-{name}.zip"));
        let (size, digest, names) = archive(&files, &asset);

        let entry = serde_json::json!({
            "engine": "unity",
            "version": version,
            "variant": name,
            "asset": format!("{RELEASES}/unity-{version}/unity-{version}-{name}.zip"),
            "size": size,
            "sha256": digest,
            "files": names,
        });

        manifest.push(entry);
        println!("built {} in {}s", asset.display(), took.as_secs());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&manifest).expect("rendering entries")
    );
}
