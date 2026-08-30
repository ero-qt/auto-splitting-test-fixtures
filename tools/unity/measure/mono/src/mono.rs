//! Mono's own embedding API, resolved out of the library once so a missing
//! export is named up front.

use std::ffi::{c_char, c_int, c_void};

/// The struct of embedding exports, and the lookup that fills it in.
macro_rules! exports {
    ($($name:ident: fn($($arg:ty),*) -> $ret:ty),* $(,)?) => {
        pub struct Mono {
            $(pub $name: unsafe extern "C" fn($($arg),*) -> $ret,)*
        }

        impl Mono {
            pub fn resolve(library: *mut c_void) -> Result<Self, String> {
                let mut missing = Vec::new();
                $(let $name = unsafe {
                    let symbol = concat!(stringify!($name), "\0");
                    let address = libc::dlsym(library, symbol.as_ptr().cast());
                    if address.is_null() {
                        missing.push(stringify!($name));
                    }
                    address
                };)*
                if !missing.is_empty() {
                    return Err(format!("not exported: {}", missing.join(", ")));
                }
                unsafe {
                    Ok(Self {
                        $($name: std::mem::transmute($name),)*
                    })
                }
            }
        }
    };
}

exports! {
    mono_set_dirs: fn(*const c_char, *const c_char) -> (),
    mono_jit_init: fn(*const c_char) -> *mut c_void,
    mono_get_corlib: fn() -> *mut c_void,
    mono_class_from_name: fn(*mut c_void, *const c_char, *const c_char) -> *mut c_void,
    mono_class_get_name: fn(*mut c_void) -> *const c_char,
    mono_class_get_namespace: fn(*mut c_void) -> *const c_char,
    mono_class_get_parent: fn(*mut c_void) -> *mut c_void,
    mono_class_get_nesting_type: fn(*mut c_void) -> *mut c_void,
    mono_class_get_fields: fn(*mut c_void, *mut *mut c_void) -> *mut c_void,
    mono_class_num_fields: fn(*mut c_void) -> c_int,
    mono_class_instance_size: fn(*mut c_void) -> c_int,
    mono_class_get_type: fn(*mut c_void) -> *mut c_void,
    mono_class_get_type_token: fn(*mut c_void) -> u32,
    mono_class_vtable: fn(*mut c_void, *mut c_void) -> *mut c_void,
    mono_class_from_mono_type: fn(*mut c_void) -> *mut c_void,
    mono_array_class_get: fn(*mut c_void, u32) -> *mut c_void,
    mono_field_get_name: fn(*mut c_void) -> *const c_char,
    mono_field_get_type: fn(*mut c_void) -> *mut c_void,
    mono_field_get_offset: fn(*mut c_void) -> u32,
    mono_type_get_type: fn(*mut c_void) -> c_int,
    mono_vtable_get_static_field_data: fn(*mut c_void) -> *mut c_void,
    mono_runtime_class_init: fn(*mut c_void) -> (),
    mono_image_get_assembly: fn(*mut c_void) -> *mut c_void,
    mono_image_get_name: fn(*mut c_void) -> *const c_char,
    mono_reflection_type_from_name: fn(*mut c_char, *mut c_void) -> *mut c_void,
}
