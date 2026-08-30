//! Where a class keeps its names, its relations, and its field array, and the
//! bits saying what kind of class it is.

use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use crate::mono::Mono;
use crate::report::{Measurement, Report};
use crate::spans::{CLASS_SPAN};
use crate::scan::{agreed, int_scan, kind_scan, readable, word_scan};
use crate::types::TypeOffsets;

/// The members every witness class agrees on. The witnesses are chosen so
/// their parents all differ, which is what stops a wrong candidate riding
/// along on a value they happen to share.
pub fn measure(report: &mut Report, mono: &Mono, witnesses: &[*mut c_void], nested: *mut c_void) {
    let by_pointer = |api: unsafe extern "C" fn(*mut c_void) -> *mut c_void| {
        agreed(witnesses.iter().map(|&class| {
            let value = unsafe { api(class) };
            word_scan(class.cast(), CLASS_SPAN, value as u64)
        }))
    };
    let by_string = |api: unsafe extern "C" fn(*mut c_void) -> *const c_char| {
        agreed(witnesses.iter().map(|&class| {
            let value = unsafe { api(class) };
            word_scan(class.cast(), CLASS_SPAN, value as u64)
        }))
    };

    let class_report = report.entry("class").or_default();
    class_report.insert(
        "name",
        Measurement::of(by_string(mono.mono_class_get_name), "name pointer not found"),
    );
    class_report.insert(
        "namespace",
        Measurement::of(
            by_string(mono.mono_class_get_namespace),
            "namespace pointer not found",
        ),
    );
    class_report.insert(
        "parent",
        Measurement::of(
            by_pointer(mono.mono_class_get_parent),
            "parent pointer not found",
        ),
    );
    class_report.insert(
        "fields",
        Measurement::of(
            agreed(witnesses.iter().map(|&class| {
                let mut iterator = ptr::null_mut();
                let first = unsafe { (mono.mono_class_get_fields)(class, &mut iterator) };
                word_scan(class.cast(), CLASS_SPAN, first as u64)
            })),
            "fields pointer not found",
        ),
    );
    class_report.insert(
        "field_count",
        Measurement::of(
            agreed(witnesses.iter().map(|&class| {
                let count = unsafe { (mono.mono_class_num_fields)(class) };
                int_scan(class.cast(), CLASS_SPAN, count)
            })),
            "field count not found",
        ),
    );
    class_report.insert(
        "instance_size",
        Measurement::of(
            agreed(witnesses.iter().take(2).map(|&class| {
                let size = unsafe { (mono.mono_class_instance_size)(class) };
                int_scan(class.cast(), CLASS_SPAN, size)
            })),
            "instance size not found",
        ),
    );

    if !nested.is_null() {
        let enclosing = unsafe { (mono.mono_class_get_nesting_type)(nested) };
        class_report.insert(
            "nested_in",
            Measurement::of(
                word_scan(nested.cast(), CLASS_SPAN, enclosing as u64),
                "nesting pointer not found",
            ),
        );
    }
}

/// The generic pair and the kind bits, both of which need instantiations to
/// measure: a generic instance points at its descriptor, whose early words
/// point back at the definition it was made from.
pub fn generics(
    report: &mut Report,
    mono: &Mono,
    corlib: *mut c_void,
    witnesses: &[*mut c_void],
    types: &TypeOffsets,
) {
    let generics = c"System.Collections.Generic";
    let definition = |name: &CStr| unsafe {
        (mono.mono_class_from_name)(corlib, generics.as_ptr(), name.as_ptr())
    };
    let instantiate = |spelled: &str| {
        let spelled = CString::new(spelled).unwrap();
        let type_ =
            unsafe { (mono.mono_reflection_type_from_name)(spelled.as_ptr() as *mut _, corlib) };
        if type_.is_null() {
            return ptr::null_mut();
        }
        unsafe { (mono.mono_class_from_mono_type)(type_) }
    };

    let list = definition(c"List`1");
    let dictionary = definition(c"Dictionary`2");
    let list_of_int = instantiate("System.Collections.Generic.List`1[[System.Int32, mscorlib]]");
    let dictionary_of_int = instantiate(
        "System.Collections.Generic.Dictionary`2[[System.Int32, mscorlib],[System.Int32, mscorlib]]",
    );
    let arrays = [
        unsafe { (mono.mono_array_class_get)(witnesses[0], 1) },
        unsafe { (mono.mono_array_class_get)(witnesses[1], 1) },
    ];
    if list.is_null()
        || dictionary.is_null()
        || list_of_int.is_null()
        || dictionary_of_int.is_null()
        || arrays.iter().any(|class| class.is_null())
    {
        return;
    }

    // Two instantiations of two definitions have to agree on the same route.
    let routes = |instance: *mut c_void, definition: *mut c_void| {
        let mut found = Vec::new();
        for at in (0..CLASS_SPAN).step_by(8) {
            let descriptor = unsafe { (instance as *const u8).add(at).cast::<*const u8>().read() };
            if descriptor as usize & 7 != 0 || !readable(descriptor, 0x20) {
                continue;
            }
            for inner in (0..0x20).step_by(8) {
                let target = unsafe { descriptor.add(inner).cast::<u64>().read_unaligned() };
                if target == definition as u64 {
                    found.push((at as u32, inner as u32));
                }
            }
        }
        found
    };
    let mut found = routes(list_of_int, list);
    let against = routes(dictionary_of_int, dictionary);
    found.retain(|route| against.contains(route));

    // A generic instance also reaches its descriptor through the two MonoType
    // structs laid inline in the class, whose data word routes the same way.
    // Those are recognized by the GenericInst kind byte sitting at the type's
    // own kind offset behind the data word.
    const GENERIC_INSTANCE: u8 = 0x15;
    if let (Some(kind), Some(data)) = (types.kind, types.data) {
        found.retain(|&(at, _)| {
            let Some(start) = at.checked_sub(data) else {
                return true;
            };
            let laid = unsafe {
                (list_of_int as *const u8)
                    .add(start as usize + kind as usize)
                    .read()
            };
            laid != GENERIC_INSTANCE
        });
    }

    let generic_report = report.entry("generic").or_default();
    match found.as_slice() {
        [(generic_class, container_class)] => {
            generic_report.insert("generic_class", Measurement::Single(*generic_class));
            generic_report.insert("container_class", Measurement::Single(*container_class));
        }
        _ => {
            generic_report.insert(
                "generic_class",
                Measurement::Missing {
                    missing: format!("{} descriptor routes", found.len()),
                },
            );
        }
    }

    // The kind bits, distinguished by classes of four kinds: plain
    // definitions, generic definitions, instances, and array classes, two of
    // each so a flag bit that happens to agree cannot survive.
    let kinds = [
        (witnesses[0], 1_u8),
        (witnesses[1], 1),
        (list, 2),
        (dictionary, 2),
        (list_of_int, 3),
        (dictionary_of_int, 3),
        (arrays[0], 5),
        (arrays[1], 5),
    ];
    report.entry("class").or_default().insert(
        "class_kind",
        Measurement::of(
            agreed(kinds.map(|(class, kind)| kind_scan(class.cast(), CLASS_SPAN, kind))),
            "kind bits not found",
        ),
    );
}
