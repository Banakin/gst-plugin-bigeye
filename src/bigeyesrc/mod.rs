// Licensed under the Apache License
// SPDX-License-Identifier: Apache-2.0

use gst::glib;
use gst::prelude::*;

mod imp;

// The public Rust wrapper type for our element
glib::wrapper! {
    pub struct BigEyeSrc(ObjectSubclass<imp::BigEyeSrc>) @extends gst_base::PushSrc, gst_base::BaseSrc, gst::Element, gst::Object;
}

// Registers the type for our element, and then registers in GStreamer under
// the name "BigEyeSrc" for being able to instantiate it via e.g.
// gst::ElementFactory::make().
pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "bigeyesrc",
        gst::Rank::NONE,
        BigEyeSrc::static_type(),
    )
}