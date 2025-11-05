use gst::glib;

mod bigeyesrc;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    bigeyesrc::register(plugin)?;
    Ok(())
}

// Define GStreamer Plugin
gst::plugin_define!(
    bigeye,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
    "MIT/X11",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY"),
    env!("BUILD_REL_DATE")
);
