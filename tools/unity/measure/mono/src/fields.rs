//! Where a field entry keeps its name, its type, and its offset, and how far
//! apart two of them sit.

use std::ffi::c_void;
use std::ptr;

use crate::mono::Mono;
use crate::report::{Measurement, Report};
use crate::spans::{FIELD_SPAN};
use crate::scan::{agreed, int_scan, word_scan};

/// Witnessed across every field of the classes given, so a candidate that
/// suits one field and not the rest cannot survive.
pub fn measure(report: &mut Report, mono: &Mono, witnesses: &[*mut c_void]) {
    // The stride is the distance between the neighbours the iterator hands
    // out; the members are witnessed across every field of every witness.
    let mut iterator = ptr::null_mut();
    let first = unsafe { (mono.mono_class_get_fields)(witnesses[0], &mut iterator) };
    let second = unsafe { (mono.mono_class_get_fields)(witnesses[0], &mut iterator) };

    let mut fields = Vec::new();
    for &class in witnesses {
        let mut iterator = ptr::null_mut();
        loop {
            let field = unsafe { (mono.mono_class_get_fields)(class, &mut iterator) };
            if field.is_null() {
                break;
            }
            fields.push(field);
        }
    }

    let field_report = report.entry("field").or_default();
    if !first.is_null() && !second.is_null() {
        field_report.insert(
            "stride",
            Measurement::Single((second as u64 - first as u64) as u32),
        );
    }
    field_report.insert(
        "name",
        Measurement::of(
            agreed(fields.iter().map(|&field| {
                let name = unsafe { (mono.mono_field_get_name)(field) };
                word_scan(field.cast(), FIELD_SPAN, name as u64)
            })),
            "field name not found",
        ),
    );
    field_report.insert(
        "type",
        Measurement::of(
            agreed(fields.iter().map(|&field| {
                let type_ = unsafe { (mono.mono_field_get_type)(field) };
                word_scan(field.cast(), FIELD_SPAN, type_ as u64)
            })),
            "field type not found",
        ),
    );
    field_report.insert(
        "offset",
        Measurement::of(
            agreed(fields.iter().map(|&field| {
                let offset = unsafe { (mono.mono_field_get_offset)(field) };
                int_scan(field.cast(), FIELD_SPAN, offset as i32)
            })),
            "field offset not found",
        ),
    );
}
