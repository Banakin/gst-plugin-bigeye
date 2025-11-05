// Licensed under the Apache License
// SPDX-License-Identifier: Apache-2.0
// For examples making GStreamer plugins with Rust, see the following repo:
// https://github.com/GStreamer/gst-plugins-rs/tree/main/tutorial

use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;

use std::sync::{Arc, Mutex};
use std::sync::LazyLock;

use uvc;

const WIDTH: i32 = 800;
const HEIGHT: i32 = 400;
const FRAMES_SECOND: i32 = 90;

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "bigeyesrc",
        gst::DebugColorFlags::empty(),
        Some("Bigscreen Beyond 2e eye tracking video source."),
    )
});

// Stream-specific state
#[allow(dead_code)]
struct State {
    info: Option<gst_video::VideoInfo>,
    // Store the entire UVC stack to keep everything alive
    uvc_context: Option<uvc::Context<'static>>,
    uvc_device: Option<uvc::Device<'static>>,
    uvc_device_handle: Option<uvc::DeviceHandle<'static>>,
    stream: Option<uvc::ActiveStream<'static, Arc<Mutex<Option<Vec<u8>>>>>>,
    
    // Store the latest frame data from the camera
    latest_frame: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Default for State {
    fn default() -> State {
        State {
            info: None,
            uvc_context: None,
            uvc_device: None,
            uvc_device_handle: None,
            stream: None,
            latest_frame: Arc::new(Mutex::new(None)),
        }
    }
}

// Struct containing all the element data
#[derive(Default)]
pub struct BigEyeSrc {
    state: Mutex<State>,
}

impl BigEyeSrc {}

// This trait registers our type with the GObject object system and
// provides the entry points for creating a new instance and setting
// up the class data
#[glib::object_subclass]
impl ObjectSubclass for BigEyeSrc {
    const NAME: &'static str = "BigEyeSrc";
    type Type = super::BigEyeSrc;
    type ParentType = gst_base::PushSrc;
}

// Implementation of glib::Object virtual methods
impl ObjectImpl for BigEyeSrc {
    // Called right after construction of a new instance
    fn constructed(&self) {
        // Call the parent class' ::constructed() implementation first
        self.parent_constructed();

        let obj = self.obj();
        // Initialize live-ness and notify the base class that
        // we'd like to operate in Time format
        obj.set_live(true);
        obj.set_format(gst::Format::Time);
    }
}

impl GstObjectImpl for BigEyeSrc {}

// Implementation of gst::Element virtual methods
impl ElementImpl for BigEyeSrc {
    // Set the element specific metadata. This information is what
    // is visible from gst-inspect-1.0 and can also be programmatically
    // retrieved from the gst::Registry after initial registration
    // without having to load the plugin in memory.
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
            gst::subclass::ElementMetadata::new(
                "Bigscreen Beyond 2e eye tracking video source.",
                "Source/Video",
                "Bigscreen Beyond 2e eye tracking video source using libuvc.",
                "Ray Foxyote <ray@foxyote.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    // Set source and sink pads.
    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            // Define Capabilities (Caps)
            // sink: None, this is a source
            // source: "image/jpeg, width=(int)800, height=(int)400, framerate=(fraction)90/1"
            let caps = gst::Caps::builder("image/jpeg")
                .field("width", WIDTH)
                .field("height", HEIGHT)
                .field("framerate", gst::Fraction::new(FRAMES_SECOND, 1))
                .build();
            
            // Make source pad template
            let src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            vec![src_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

    // Called whenever the state of the element should be changed. This allows for
    // starting up the element, allocating/deallocating resources or shutting down
    // the element again.
    fn change_state(
        &self,
        transition: gst::StateChange,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        // Configure live'ness once here just before starting the source
        if let gst::StateChange::ReadyToPaused = transition {
            self.obj().set_live(true);
        }

        // Call the parent class' implementation of ::change_state()
        self.parent_change_state(transition)
    }
}

// Implementation of gst_base::BaseSrc virtual methods
impl BaseSrcImpl for BigEyeSrc {
    // Called whenever the input/output caps are changing
    fn set_caps(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        let info = gst_video::VideoInfo::from_caps(caps).map_err(|_| {
            gst::loggable_error!(CAT, "Failed to build `VideoInfo` from caps {}", caps)
        })?;

        gst::debug!(CAT, imp = self, "Configuring for caps {}", caps);

        let mut state = self.state.lock().unwrap();
        state.info = Some(info);
        drop(state);

        Ok(())
    }

    // Called when starting, so we can initialize the stream
    // This initializes the UVC context, then gets the device, opens it, creates the stream, and then starts it
    // Box::leak is a standard function in Rust. It consumes the Box and leaks it onto the heap, so it lives for the duration of the program.
    // Read more at https://doc.rust-lang.org/std/boxed/struct.Box.html
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        gst::info!(CAT, imp = self, "Starting video capture");

        let mut state = self.state.lock().unwrap();
        
        // Initialize context
        let ctx = Box::leak(Box::new(uvc::Context::new().map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::OpenRead,
                ["Could not create context: {:?}", e]
            )
        })?));
        gst::info!(CAT, imp = self, "Context created");

        // Get a BSB2E device using Vendor ID and Product ID
        let dev = Box::leak(Box::new(ctx.find_device(Some(0x35bd), Some(0x0202), None).map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::NotFound,
                ["Could not find device: {:?}", e]
            )
        })?));
        gst::info!(CAT, imp = self, "Device found");

        // Open the device
        let devh = Box::leak(Box::new(dev.open().map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::OpenRead,
                ["Could not open device: {:?}", e]
            )
        })?));
        gst::info!(CAT, imp = self, "Device opened");

        // Configure for MJPEG format at 800x400@90fps
        let format = uvc::StreamFormat {
            width: (WIDTH as u32),
            height: (HEIGHT as u32),
            fps: (FRAMES_SECOND as u32),
            format: uvc::FrameFormat::MJPEG,
        };

        // Get stream handle
        let streamh = Box::leak(Box::new(devh.get_stream_handle_with_format(format).map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::Settings,
                ["Could not open stream with format: {:?}", e]
            )
        })?));
        gst::info!(CAT, imp = self, "Stream handle obtained");

        // Start the stream with a callback that stores frame data
        let latest_frame = state.latest_frame.clone();   
        let stream = streamh
            .start_stream(
                move |frame, context| {
                    // Store the frame data as bytes
                    let mut locked = context.lock().unwrap();
                    *locked = Some(frame.to_bytes().to_vec());
                },
                latest_frame.clone(),
            )
            .map_err(|e| {
                gst::error_msg!(
                    gst::ResourceError::OpenRead,
                    ["Could not start stream: {:?}", e]
                )
            })?;

        gst::info!(CAT, imp = self, "Stream started successfully");
        eprintln!("Stream started, waiting for frames...");

        state.stream = Some(stream);

        drop(state);

        gst::info!(CAT, imp = self, "Started video capture");
        Ok(())
    }

    // Called when shutting down the element
    // Stops the UVC stream and clears the state
    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        gst::info!(CAT, imp = self, "Stopping video capture");
        
        let mut state = self.state.lock().unwrap();
        
        // Stop the stream (will be dropped automatically)
        if let Some(stream) = state.stream.take() {
            stream.stop();
        }
        
        // Clear the latest frame
        *state.latest_frame.lock().unwrap() = None;
        
        drop(state);

        gst::info!(CAT, imp = self, "Stopped video capture");
        Ok(())
    }

    fn is_seekable(&self) -> bool {
        false
    }
}

impl PushSrcImpl for BigEyeSrc {
    // Creates the video buffer
    fn create(
        &self,
        _buffer: Option<&mut gst::BufferRef>,
    ) -> Result<CreateSuccess, gst::FlowError> {
        // Get latest frame
        let state = self.state.lock().unwrap();
        let latest_frame = state.latest_frame.clone();
        drop(state);  // Release the state lock early

        // Get the latest frame from the camera
        // Wait for a frame to be available with timeout
        let frame_data = {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(5);
            
            loop {
                let mut latest = latest_frame.lock().unwrap();
                match latest.take() {
                    Some(data) => {
                        gst::trace!(CAT, imp = self, "Got frame data of {} bytes", data.len());
                        break data;
                    }
                    None => {
                        // No frame available yet, check timeout
                        if start.elapsed() > timeout {
                            drop(latest);
                            gst::error!(CAT, imp = self, "No frame available, waiting...");
                            return Err(gst::FlowError::Eos);
                        }
                        // Wait a bit and retry
                        drop(latest);
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
        };

        // Create a GStreamer buffer with the frame data
        let mut buffer = gst::Buffer::from_slice(frame_data);
        {
            let buffer_ref = buffer.get_mut().unwrap();
            
            // For live sources, use the current running time for timestamping
            let obj = self.obj();
            if let Some(clock) = obj.clock() {
                if let Some(base_time) = obj.base_time() {
                    let now = clock.time();
                    if let Some(pts) = now.checked_sub(base_time) {
                        buffer_ref.set_pts(pts);
                    }
                }
            }
            
            // Set duration based on framerate
            let duration = gst::ClockTime::SECOND / (FRAMES_SECOND as u64);
            buffer_ref.set_duration(duration);
        }

        gst::log!(CAT, imp = self, "Produced buffer {:?}", buffer);

        Ok(CreateSuccess::NewBuffer(buffer))
    }
}