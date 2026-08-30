//! Where an assembly keeps its image and its name, and the hash table an
//! image resolves its classes through.

use std::ffi::c_void;

use crate::mono::Mono;
use crate::report::{Measurement, Report};
use crate::spans::{ASSEMBLY_SPAN, CLASS_SPAN, IMAGE_SPAN};
use crate::scan::{readable, word_scan};

/// Corlib's own assembly and image, which is where the class cache is found.
pub fn measure(report: &mut Report, mono: &Mono, corlib: *mut c_void, witnesses: &[*mut c_void]) {
    // The assembly words: corlib's assembly carries the image pointer, and
    // its name struct sits inline at the aname offset.
    let assembly = unsafe { (mono.mono_image_get_assembly)(corlib) };
    let assembly_report = report.entry("assembly").or_default();
    assembly_report.insert(
        "image",
        Measurement::of(
            word_scan(assembly.cast(), ASSEMBLY_SPAN, corlib as u64),
            "image pointer not found",
        ),
    );
    // The name struct starts with the name string, so the assembly word that
    // dereferences to corlib's own name is where the struct sits inline.
    let mut names = Vec::new();
    for at in (0..ASSEMBLY_SPAN).step_by(8) {
        let text = unsafe { (assembly as *const u8).add(at).cast::<*const u8>().read() };
        if !readable(text, b"mscorlib\0".len()) {
            continue;
        }
        let head = unsafe { std::slice::from_raw_parts(text, b"mscorlib\0".len()) };
        if head == b"mscorlib\0" {
            names.push(at as u32);
        }
    }
    assembly_report.insert("aname", Measurement::of(names, "corlib name not found"));

    // The image name, informational: the tables read names through aname.
    let image_name = unsafe { (mono.mono_image_get_name)(corlib) };
    report.entry("image").or_default().insert(
        "name",
        Measurement::of(
            word_scan(corlib.cast(), IMAGE_SPAN, image_name as u64),
            "image name not found",
        ),
    );

    // The class cache: the one image word whose pointed struct holds a table
    // and size through which every witness class is reachable by its token,
    // chained through the same class word.
    let cache = measure_cache(mono, corlib, witnesses);
    let image_report = report.entry("image").or_default();
    match cache {
        Some(cache) => {
            image_report.insert("class_cache", Measurement::Single(cache.at));
            let hash_report = report.entry("hash_table").or_default();
            hash_report.insert("size", Measurement::Single(cache.size));
            hash_report.insert("table", Measurement::Single(cache.table));
            report
                .entry("class")
                .or_default()
                .insert("next_class_cache", Measurement::Single(cache.chain));
        }
        None => {
            image_report.insert(
                "class_cache",
                Measurement::Missing {
                    missing: "no cache shape matched".into(),
                },
            );
        }
    }

}


struct Cache {
    at: u32,
    size: u32,
    table: u32,
    chain: u32,
}

/// Searches the image for the internal hash table: a size, a bucket array
/// holding every witness at its token's bucket, and the chain offset the
/// whole table is consistent with. A chain candidate is right only when
/// every node of every bucket hashes back to the bucket it sits in, and at
/// least one real link was followed to see it.
fn measure_cache(
    mono: &Mono,
    image: *mut c_void,
    witnesses: &[*mut c_void],
) -> Option<Cache> {
    let tokens: Vec<u32> = witnesses
        .iter()
        .map(|&class| unsafe { (mono.mono_class_get_type_token)(class) })
        .collect();

    for at in (0..IMAGE_SPAN).step_by(8) {
        // The table pointer and size are probed at the handful of layouts the
        // struct has ever had: size as an i32 or a word, table behind it.
        for (size_at, table_at) in [(0x18_u32, 0x20_u32), (0x0C, 0x14), (0x18, 0x28)] {
            let base = unsafe { (image as *const u8).add(at) };
            let size = unsafe { base.add(size_at as usize).cast::<i32>().read_unaligned() };
            if !(1..1 << 20).contains(&size) {
                continue;
            }
            let size = size as u32;
            let table = unsafe {
                base.add(table_at as usize)
                    .cast::<*const *mut c_void>()
                    .read_unaligned()
            };
            if table as usize & 7 != 0 || !readable(table.cast(), size as usize * 8) {
                continue;
            }

            // Every witness has to be somewhere in its own bucket for this
            // to be the class cache at all; heads alone anchor that check.
            let heads: Vec<*mut c_void> = (0..size)
                .map(|bucket| unsafe { table.add(bucket as usize).read() })
                .collect();
            if !witnesses
                .iter()
                .zip(&tokens)
                .all(|(_, &token)| !heads[(token % size) as usize].is_null())
            {
                continue;
            }

            'chain: for chain in (0..0x140_u32).step_by(8) {
                let mut followed = 0_u32;
                let mut witnessed = 0;
                for (bucket, &head) in heads.iter().enumerate() {
                    let mut cursor = head;
                    let mut steps = 0;
                    while !cursor.is_null() {
                        // The token read walks class internals, so the whole
                        // class span has to be readable, not just the link.
                        if cursor as usize & 7 != 0
                            || !readable(cursor.cast(), CLASS_SPAN.max(chain as usize + 8))
                        {
                            continue 'chain;
                        }
                        let token = unsafe { (mono.mono_class_get_type_token)(cursor) };
                        if token % size != bucket as u32 {
                            continue 'chain;
                        }
                        if witnesses.contains(&cursor) {
                            witnessed += 1;
                        }
                        steps += 1;
                        if steps > 1 {
                            followed += 1;
                        }
                        if steps > 1024 {
                            continue 'chain;
                        }
                        cursor = unsafe {
                            (cursor as *const u8)
                                .add(chain as usize)
                                .cast::<*mut c_void>()
                                .read()
                        };
                    }
                }
                if followed > 0 && witnessed == witnesses.len() {
                    return Some(Cache {
                        at: at as u32,
                        size: size_at,
                        table: table_at,
                        chain,
                    });
                }
            }
        }
    }

    None
}
