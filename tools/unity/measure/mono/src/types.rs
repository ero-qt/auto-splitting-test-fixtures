//! Where a MonoType keeps the class it names and the byte saying what kind of
//! type it is.

use std::ffi::c_void;

use crate::mono::Mono;
use crate::report::{Measurement, Report};
use crate::spans::{TYPE_SPAN};
use crate::scan::{agreed, byte_scan, word_scan};

/// Where the kind byte and the data word sit, when each is settled. The
/// generic walk needs both to tell a descriptor from a type laid inline.
pub struct TypeOffsets {
    pub kind: Option<u32>,
    pub data: Option<u32>,
}

/// `named` has to be a plain class, whose type keeps that class in its data
/// word. The kind byte needs witnesses whose element kinds are not all the
/// same, or every byte they share survives.
pub fn measure(
    report: &mut Report,
    mono: &Mono,
    witnesses: &[*mut c_void],
    named: *mut c_void,
) -> TypeOffsets {
    let types: Vec<*mut c_void> = witnesses
        .iter()
        .map(|&class| unsafe { (mono.mono_class_get_type)(class) })
        .collect();

    let kind = agreed(types.iter().map(|&type_| {
        let kind = unsafe { (mono.mono_type_get_type)(type_) } as u8;
        byte_scan(type_.cast(), TYPE_SPAN, kind)
    }));
    let data = word_scan(
        unsafe { (mono.mono_class_get_type)(named) }.cast(),
        TYPE_SPAN,
        named as u64,
    );

    let settled = |candidates: &[u32]| match candidates {
        [only] => Some(*only),
        _ => None,
    };
    let words = TypeOffsets {
        kind: settled(&kind),
        data: settled(&data),
    };

    let type_report = report.entry("type").or_default();
    type_report.insert("kind", Measurement::of(kind, "type kind not found"));
    type_report.insert("data", Measurement::of(data, "type data not found"));

    words
}
