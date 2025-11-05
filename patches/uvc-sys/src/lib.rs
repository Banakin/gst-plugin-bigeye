#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unnecessary_transmutes)]

// The anonymous union type is blacklisted and not included in bindings
#[repr(C)]
pub union uvc_format_desc_union {
    _placeholder: u64,
}

include!(concat!(env!("OUT_DIR"), "/uvc_bindings.rs"));
