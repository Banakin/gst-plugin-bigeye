// Copyright (C) 2018 Sebastian Dröge <sebastian@centricular.com>
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::base_src::CreateSuccess;
use gst_base::subclass::prelude::*;

use std::sync::{Arc, Mutex};
use std::sync::LazyLock;

use uvc;

// This module contains the private implementation details of our element

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "bigeyesrc",
        gst::DebugColorFlags::empty(),
        Some("UVC Webcam Video Source"),
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
    frame_count: u64,
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
            frame_count: 0,
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
                "UVC Webcam Source",
                "Source/Video",
                "Captures video from UVC webcams using libuvc",
                "Ray Foxyote <ray@foxyote.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    // Create and add pad templates for our source pad
    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            // Support MJPEG format which is what we're actually outputting
            let caps = gst::Caps::builder("image/jpeg")
                .field("width", 800)
                .field("height", 400)
                .field("framerate", gst::Fraction::new(90, 1))
                .build();
            
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

    // Called when starting, so we can initialize the UVC stream
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        gst::info!(CAT, imp = self, "Starting UVC capture");

        let mut state = self.state.lock().unwrap();
        
        // Initialize UVC context and device
        // We need to leak these to get 'static lifetime
        let ctx = Box::leak(Box::new(uvc::Context::new().map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::OpenRead,
                ["Could not create UVC context: {:?}", e]
            )
        })?));

        gst::info!(CAT, imp = self, "UVC context created");

        // Get a BSB2E
        let dev = Box::leak(Box::new(ctx.find_device(Some(0x35bd), Some(0x0202), None).map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::NotFound,
                ["Could not find UVC device: {:?}", e]
            )
        })?));

        gst::info!(CAT, imp = self, "UVC device found");

        // Open the device
        let devh = Box::leak(Box::new(dev.open().map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::OpenRead,
                ["Could not open UVC device: {:?}", e]
            )
        })?));

        gst::info!(CAT, imp = self, "UVC device opened");

        // Configure for YUYV format at 640x480@30fps
        let format = uvc::StreamFormat {
            width: 800,
            height: 400,
            fps: 90,
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
        let frame_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let frame_counter_cb = frame_counter.clone();
        
        let stream = streamh
            .start_stream(
                move |frame, context| {
                    // Store the frame data as bytes
                    let count = frame_counter_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let mut locked = context.lock().unwrap();
                    *locked = Some(frame.to_bytes().to_vec());
                },
                latest_frame.clone(),
            )
            .map_err(|e| {
                gst::error_msg!(
                    gst::ResourceError::OpenRead,
                    ["Could not start UVC stream: {:?}", e]
                )
            })?;

        gst::info!(CAT, imp = self, "UVC stream started successfully");
        eprintln!("UVC stream started, waiting for frames...");

        state.stream = Some(stream);
        state.frame_count = 0;

        drop(state);

        gst::info!(CAT, imp = self, "Started UVC capture");
        Ok(())
    }

    // Called when shutting down the element
    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        gst::info!(CAT, imp = self, "Stopping UVC capture");
        
        let mut state = self.state.lock().unwrap();
        
        // Stop the stream (will be dropped automatically)
        if let Some(stream) = state.stream.take() {
            stream.stop();
        }
        
        // Clear the latest frame
        *state.latest_frame.lock().unwrap() = None;
        state.frame_count = 0;
        
        drop(state);

        gst::info!(CAT, imp = self, "Stopped UVC capture");
        Ok(())
    }

    fn is_seekable(&self) -> bool {
        false
    }
}

impl PushSrcImpl for BigEyeSrc {
    // Creates the video buffers from UVC frames
    fn create(
        &self,
        _buffer: Option<&mut gst::BufferRef>,
    ) -> Result<CreateSuccess, gst::FlowError> {
        let state = self.state.lock().unwrap();
        
        let _info = match state.info {
            None => {
                gst::element_imp_error!(self, gst::CoreError::Negotiation, ["Have no caps yet"]);
                return Err(gst::FlowError::NotNegotiated);
            }
            Some(ref info) => info.clone(),
        };

        let frame_count = state.frame_count;
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
                            gst::error!(CAT, imp = self, "Timeout waiting for frames from UVC device");
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
            let fps = 30;
            let duration = gst::ClockTime::SECOND / fps;
            buffer_ref.set_duration(duration);
        }
        
        let mut state = self.state.lock().unwrap();
        state.frame_count += 1;
        drop(state);

        gst::log!(CAT, imp = self, "Produced buffer {:?}", buffer);

        Ok(CreateSuccess::NewBuffer(buffer))
    }
}