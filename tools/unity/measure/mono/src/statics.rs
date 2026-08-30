//! How a class reaches its static data, which pins the vtable layout at the
//! same time: where the method table starts, and where the class keeps the
//! size of it.

use std::ffi::c_void;

use crate::mono::Mono;
use crate::report::{Measurement, Report};
use crate::spans::{CLASS_SPAN, VTABLE_HEAD, VTABLE_SPAN};
use crate::scan::{agreed, readable, word_scan};

/// The route from a class to its static data, measured from classes that
/// store statics so the pointer exists to be found.
pub fn measure(report: &mut Report, mono: &Mono, domain: *mut c_void, witnesses: &[*mut c_void]) {
    let vtables: Vec<*mut c_void> = witnesses
        .iter()
        .map(|&class| {
            let vtable = unsafe { (mono.mono_class_vtable)(domain, class) };
            if !vtable.is_null() {
                // Static data is allocated at initialization, not creation.
                unsafe { (mono.mono_runtime_class_init)(vtable) };
            }
            vtable
        })
        .collect();


    // The static data pointer sits one slot past the method table, so
    // finding it pins where the method table starts and where the class
    // keeps its vtable size, together: statics == *(vtable + table +
    // size * word), for the size read at the same class word everywhere.
    let slots = |class: *mut c_void, vtable: *mut c_void| {
        let statics = unsafe { (mono.mono_vtable_get_static_field_data)(vtable) };
        let mut found = Vec::new();
        if statics.is_null() {
            return found;
        }
        for table in (0..VTABLE_SPAN as u32).step_by(8) {
            for size_at in (0..CLASS_SPAN as u32).step_by(4) {
                let size = unsafe {
                    (class as *const u8)
                        .add(size_at as usize)
                        .cast::<i32>()
                        .read_unaligned()
                };
                if !(1..4096).contains(&size) {
                    continue;
                }
                let slot = unsafe {
                    (vtable as *const u8).add(table as usize + size as usize * 8)
                };
                if !readable(slot, 8) {
                    continue;
                }
                if unsafe { slot.cast::<u64>().read_unaligned() } == statics as u64 {
                    found.push((table, size_at));
                }
            }
        }
        found
    };
    // One Mono layout keeps the data pointer in the vtable's head words and
    // the other never does, so a hit here settles which layout this is.
    let direct_rounds: Vec<Vec<u32>> = vtables
        .iter()
        .filter(|vtable| !vtable.is_null())
        .map(|&vtable| {
            let statics = unsafe { (mono.mono_vtable_get_static_field_data)(vtable) };
            if statics.is_null() {
                return Vec::new();
            }
            word_scan(vtable.cast(), VTABLE_HEAD, statics as u64)
        })
        .filter(|round| !round.is_empty())
        .collect();
    let direct = match direct_rounds.len() {
        0 | 1 => Vec::new(),
        _ => agreed(direct_rounds),
    };
    if let [only] = direct.as_slice() {
        report
            .entry("vtable")
            .or_default()
            .insert("data", Measurement::Single(*only));
    } else {
        let rounds: Vec<Vec<(u32, u32)>> = witnesses
            .iter()
            .zip(&vtables)
            .filter(|(_, vtable)| !vtable.is_null())
            .map(|(&class, &vtable)| slots(class, vtable))
            .filter(|routes| !routes.is_empty())
            .collect();
        let mut found = match rounds.first() {
            Some(first) if rounds.len() >= 2 => first.clone(),
            _ => Vec::new(),
        };
        for round in rounds.iter().skip(1) {
            found.retain(|route| round.contains(route));
        }
        match found.as_slice() {
            [(table, size_at)] => {
                report
                    .entry("vtable")
                    .or_default()
                    .insert("vtable", Measurement::Single(*table));
                report
                    .entry("class")
                    .or_default()
                    .insert("vtable_size", Measurement::Single(*size_at));
            }
            _ => {
                report.entry("vtable").or_default().insert(
                    "vtable",
                    Measurement::Missing {
                        missing: format!("{} statics slots", found.len()),
                    },
                );
            }
        }
    }

    let routes = |class: *mut c_void, vtable: *mut c_void| {
        let mut found = Vec::new();
        for at in (0..CLASS_SPAN).step_by(8) {
            let info = unsafe { (class as *const u8).add(at).cast::<*const u8>().read() };
            if info as usize & 7 != 0 || !readable(info, 0x20) {
                continue;
            }
            for inner in (0..0x20).step_by(8) {
                let target = unsafe { info.add(inner).cast::<u64>().read_unaligned() };
                if target == vtable as u64 {
                    found.push((at as u32, inner as u32));
                }
            }
        }
        found
    };
    let mut found = routes(witnesses[0], vtables[0]);
    for (&class, &vtable) in witnesses.iter().zip(&vtables).skip(1) {
        if vtable.is_null() {
            continue;
        }
        let against = routes(class, vtable);
        found.retain(|route| against.contains(route));
    }
    report.entry("vtable").or_default().insert(
        "runtime_info",
        match found.as_slice() {
            [(at, _)] => Measurement::Single(*at),
            _ => Measurement::Missing {
                missing: format!("{} vtable routes", found.len()),
            },
        },
    );
}
