//! Measures where Mono keeps the members of its own metadata structures, from
//! the exact library a game ships: it is loaded with `dlopen`, booted through
//! its own embedding exports, and every offset is found by scanning live
//! struct bytes for values the API hands back. A candidate that more than one
//! witness agrees on is the measurement; anything ambiguous is reported as
//! such rather than guessed.

mod classes;
mod fields;
mod images;
mod mono;
mod report;
mod scan;
mod spans;
mod statics;
mod types;

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::{fs, process};

use clap::Parser;

use mono::Mono;
use report::{Measurement, Report};

#[derive(Parser)]
struct Args {
    /// Path to the editor binary, whose class library the runtime boots against
    #[arg(short, long)]
    editor: PathBuf,

    /// Path to the Mono library out of a built player
    #[arg(short, long)]
    library: PathBuf,

    /// File the measurements are written to
    #[arg(short, long)]
    out: PathBuf,
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    process::exit(1);
}

/// Where an editor keeps Mono, which each platform puts somewhere else. The
/// `etc` directory beside it is what tells the right one from a namesake.
fn mono_dir(editor: &Path) -> Option<PathBuf> {
    let beside = editor.parent()?;
    let candidates = [
        // A Mac editor, whose binary sits in the app bundle's MacOS directory
        // with Mono among the bundle's resources.
        beside
            .parent()
            .map(|contents| contents.join("Resources/Scripting/MonoBleedingEdge")),
        // A Windows or Linux editor, whose binary sits beside Data.
        Some(beside.join("Data/MonoBleedingEdge")),
        Some(beside.join("Data/Mono")),
    ];

    candidates
        .into_iter()
        .flatten()
        .find(|path| path.join("etc").is_dir())
}

/// The class library a player's own runtime matches. Editors name the profile
/// after the platform once they ship more than one of them.
fn assemblies(mono: &Path) -> Option<PathBuf> {
    let platform = if cfg!(target_os = "macos") {
        "unityjit-macos"
    } else if cfg!(target_os = "windows") {
        "unityjit-win32"
    } else {
        "unityjit-linux"
    };

    [platform, "unityjit", "unity", "2.0"]
        .into_iter()
        .map(|profile| mono.join("lib/mono").join(profile))
        .find(|path| path.join("mscorlib.dll").is_file())
}

fn main() {
    let args = Args::parse();
    let mono_dir = mono_dir(&args.editor)
        .unwrap_or_else(|| fail("no Mono directory under that editor"));
    let assemblies = assemblies(&mono_dir)
        .unwrap_or_else(|| fail("no class library with an mscorlib under that editor"));

    // Mono reads MONO_PATH as well as the directories it is told, and both
    // want the same class library. This has to happen before anything is
    // loaded, because setting it moves the block a loaded library would
    // already be reading from.
    std::env::set_var("MONO_PATH", &assemblies);

    let path = CString::new(args.library.to_string_lossy().as_ref()).unwrap();
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_GLOBAL) };
    if handle.is_null() {
        let why = unsafe { CStr::from_ptr(libc::dlerror()) };
        fail(&format!("dlopen: {}", why.to_string_lossy()));
    }

    let mono = Mono::resolve(handle).unwrap_or_else(|why| fail(&why));

    let assemblies = CString::new(assemblies.to_string_lossy().as_ref()).unwrap();
    let config = CString::new(mono_dir.join("etc").to_string_lossy().as_ref()).unwrap();
    let domain = unsafe {
        (mono.mono_set_dirs)(assemblies.as_ptr(), config.as_ptr());
        (mono.mono_jit_init)(c"measure".as_ptr())
    };
    if domain.is_null() {
        fail("the runtime did not start");
    }

    let corlib = unsafe { (mono.mono_get_corlib)() };
    if corlib.is_null() {
        fail("the runtime started without a class library");
    }
    let class = |space: &CStr, name: &CStr| unsafe {
        (mono.mono_class_from_name)(corlib, space.as_ptr(), name.as_ptr())
    };

    // Their parents all differ, so a candidate that is right for one of them
    // and wrong for the rest cannot survive the intersection.
    let witnesses = [
        class(c"System", c"String"),
        class(c"System", c"Exception"),
        class(c"System", c"Math"),
        class(c"System", c"Int32"),
        class(c"System", c"ArgumentException"),
    ];
    let with_statics = [witnesses[0], witnesses[1], class(c"System", c"DateTime")];
    if witnesses.iter().chain(&with_statics).any(|class| class.is_null()) {
        fail("the class library did not answer for every witness");
    }

    let mut report = Report::new();
    classes::measure(
        &mut report,
        &mono,
        &witnesses,
        class(c"System", c"Environment/SpecialFolder"),
    );
    fields::measure(&mut report, &mono, &witnesses[..2]);
    let types = types::measure(&mut report, &mono, &witnesses[..3], witnesses[1]);
    classes::generics(&mut report, &mono, corlib, &witnesses, &types);
    statics::measure(&mut report, &mono, domain, &with_statics);
    images::measure(&mut report, &mono, corlib, &witnesses);

    let rendered = serde_json::to_string_pretty(&report).expect("rendering the measurements");
    if let Err(why) = fs::write(&args.out, rendered) {
        fail(&format!("writing {}: {why}", args.out.display()));
    }

    let members = report.values().flat_map(|section| section.values());
    let settled = members
        .clone()
        .filter(|member| matches!(member, Measurement::Single(_)))
        .count();
    println!(
        "measured {settled} of {} members to {}",
        members.count(),
        args.out.display(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};

    /// A directory of its own, so tests laying out editors do not collide.
    fn scratch() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);

        let at = std::env::temp_dir().join(format!(
            "measure-mono-{}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&at).expect("making a scratch directory");
        at
    }

    fn lay(root: &Path, mono: &str, profile: &str) -> PathBuf {
        let mono = root.join(mono);
        fs::create_dir_all(mono.join("etc")).expect("laying an etc directory");
        let library = mono.join("lib/mono").join(profile);
        fs::create_dir_all(&library).expect("laying a class library");
        fs::write(library.join("mscorlib.dll"), []).expect("laying an mscorlib");
        mono
    }

    // A Mac editor's binary sits inside the app bundle, with Mono beside the
    // bundle's other resources rather than under a Data directory.
    #[test]
    fn mac_editors_are_found_through_their_bundle() {
        let root = scratch();
        let mono = lay(
            &root,
            "Unity.app/Contents/Resources/Scripting/MonoBleedingEdge",
            "unityjit-macos",
        );

        let bundled = root.join("Unity.app/Contents/MacOS");
        fs::create_dir_all(&bundled).expect("laying a bundle");

        assert_eq!(mono_dir(&bundled.join("Unity")), Some(mono));
    }

    #[test]
    fn windows_and_linux_editors_are_found_beside_their_data() {
        let root = scratch();
        let mono = lay(&root, "Editor/Data/MonoBleedingEdge", "unityjit-linux");

        assert_eq!(mono_dir(&root.join("Editor/Unity.exe")), Some(mono));
    }

    // The etc directory is what tells Mono's own from a namesake, so a tree
    // without one is not an answer.
    #[test]
    fn a_directory_without_a_config_is_not_mono() {
        let root = scratch();
        fs::create_dir_all(root.join("Editor/Data/MonoBleedingEdge/lib"))
            .expect("laying a directory");

        assert_eq!(mono_dir(&root.join("Editor/Unity.exe")), None);
        assert_eq!(mono_dir(&root.join("nothing/here/Unity")), None);
    }

    // Editors that ship one profile per platform name it, and older ones ship
    // a single unnamed one. Both have to answer.
    #[test]
    fn class_libraries_fall_back_through_their_profiles() {
        let root = scratch();
        let named = lay(&root, "named", "unityjit-linux");
        let plain = lay(&root, "plain", "unityjit");
        let old = lay(&root, "old", "unity");

        if cfg!(target_os = "linux") {
            assert_eq!(
                assemblies(&named),
                Some(named.join("lib/mono/unityjit-linux")),
            );
        }
        assert_eq!(assemblies(&plain), Some(plain.join("lib/mono/unityjit")));
        assert_eq!(assemblies(&old), Some(old.join("lib/mono/unity")));
    }

    #[test]
    fn a_class_library_without_an_mscorlib_is_not_one() {
        let root = scratch();
        fs::create_dir_all(root.join("lib/mono/unityjit")).expect("laying a directory");

        assert_eq!(assemblies(&root), None);
    }
}
