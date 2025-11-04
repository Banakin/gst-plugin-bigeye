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

use std::ops::Rem;
use std::sync::Mutex;

use std::sync::LazyLock;

use uvc;

// This module contains the private implementation details of our element

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "bigeyesrc",
        gst::DebugColorFlags::empty(),
        Some("Rust Sine Wave Source"),
    )
});

// Stream-specific state, i.e. audio format configuration
// and sample offset
struct State {
    context: uvc::Context,
    stream: uvc::StreamHandle,
}

impl Default for State {
    fn default() -> State {
        State {
            context: None,
            stream: None,
        }
    }
}


// Struct containing all the element data
#[derive(Default)]
pub struct BigEyeSrc {
    state: Mutex<State>,
}

impl BigEyeSrc {
    fn load_stream() {
        let ctx = uvc::Context::new().expect("Could not get context");

        // Get a default device
        let dev = ctx
            .find_device(None, None, None)
            .expect("Could not find device");

        // The device must be opened to create a handle to the device
        let devh = dev.open().expect("Could not open device");

        // Most webcams support this format
        let format = uvc::StreamFormat {
            width: 640,
            height: 480,
            fps: 30,
            format: uvc::FrameFormat::YUYV,
        };

        // Get the necessary stream information and return
        devh
            .get_stream_handle_with_format(format)
            .expect("Could not open a stream with this format");
    }

    fn get_frame<F: Float + FromByteSlice>(
        data: &mut [u8],
        accumulator_ref: &mut f64,
        freq: u32,
        rate: u32,
        channels: u32,
        vol: f64,
    ) {
        use std::f64::consts::PI;

        // Reinterpret our byte-slice as a slice containing elements of the type
        // we're interested in. GStreamer requires for raw audio that the alignment
        // of memory is correct, so this will never ever fail unless there is an
        // actual bug elsewhere.
        let data = data.as_mut_slice_of::<F>().unwrap();

        // Convert all our parameters to the target type for calculations
        let vol: F = NumCast::from(vol).unwrap();
        let freq = freq as f64;
        let rate = rate as f64;
        let two_pi = 2.0 * PI;

        // We're carrying a accumulator with up to 2pi around instead of working
        // on the sample offset. High sample offsets cause too much inaccuracy when
        // converted to floating point numbers and then iterated over in 1-steps
        let mut accumulator = *accumulator_ref;
        let step = two_pi * freq / rate;

        for chunk in data.chunks_exact_mut(channels as usize) {
            let value = vol * F::sin(NumCast::from(accumulator).unwrap());
            for sample in chunk {
                *sample = value;
            }

            accumulator += step;
            if accumulator >= two_pi {
                accumulator -= two_pi;
            }
        }

        *accumulator_ref = accumulator;
    }
}

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
                "Sine Wave Source",
                "Video Source",
                "Creates a sine wave",
                "Ray Foxyote <ray@foxyote.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    // Create and add pad templates for our sink and source pad. These
    // are later used for actually creating the pads and beforehand
    // already provide information to GStreamer about all possible
    // pads that could exist for this type.
    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            // On the src pad, we can produce F32/F64 with any sample rate
            // and any number of channels
            let caps = gst_video::VideoCapsBuilder::new_interleaved()
                .format_list([gst_video::VIDEO_FORMATS_ALL])
                .build();
            // The src pad template must be named "src" for basesrc
            // and specific a pad that is always there
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
    // Called whenever the input/output caps are changing, i.e. in the very beginning before data
    // flow happens and whenever the situation in the pipeline is changing. All buffers after this
    // call have the caps given here.
    //
    // We simply remember the resulting VideoInfo from the caps to be able to use this for knowing
    // the sample rate, etc. when creating buffers
    fn set_caps(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        use std::f64::consts::PI;

        let info = gst_video::VideoInfo::from_caps(caps).map_err(|_| {
            gst::loggable_error!(CAT, "Failed to build `VideoInfo` from caps {}", caps)
        })?;

        gst::debug!(CAT, imp = self, "Configuring for caps {}", caps);

        self.obj()
            .set_blocksize(info.bpf() * (self.settings.lock().unwrap()).samples_per_buffer);

        let settings = *self.settings.lock().unwrap();
        let mut state = self.state.lock().unwrap();

        // If we have no caps yet, any old sample_offset and sample_stop will be
        // in nanoseconds
        let old_rate = match state.info {
            Some(ref info) => info.rate() as u64,
            None => *gst::ClockTime::SECOND,
        };

        // Update sample offset and accumulator based on the previous values and the
        // sample rate change, if any
        let old_sample_offset = state.sample_offset;
        let sample_offset = old_sample_offset
            .mul_div_floor(info.rate() as u64, old_rate)
            .unwrap();

        let old_sample_stop = state.sample_stop;
        let sample_stop =
            old_sample_stop.map(|v| v.mul_div_floor(info.rate() as u64, old_rate).unwrap());

        let accumulator =
            (sample_offset as f64).rem(2.0 * PI * (settings.freq as f64) / (info.rate() as f64));

        *state = State {
            info: Some(info),
            sample_offset,
            sample_stop,
            accumulator,
        };

        drop(state);

        let _ = self
            .obj()
            .post_message(gst::message::Latency::builder().src(&*self.obj()).build());

        Ok(())
    }

    // Called when starting, so we can initialize all stream-related state to its defaults
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        // Reset state
        *self.state.lock().unwrap() = Default::default();
        self.unlock_stop()?;

        gst::info!(CAT, imp = self, "Started");

        Ok(())
    }

    // Called when shutting down the element so we can release all stream-related state
    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        // Reset state
        *self.state.lock().unwrap() = Default::default();
        self.unlock()?;

        gst::info!(CAT, imp = self, "Stopped");

        Ok(())
    }

    fn query(&self, query: &mut gst::QueryRef) -> bool {
        use gst::QueryViewMut;

        match query.view_mut() {
            // In Live mode we will have a latency equal to the number of samples in each buffer.
            // We can't output samples before they were produced, and the last sample of a buffer
            // is produced that much after the beginning, leading to this latency calculation
            QueryViewMut::Latency(q) => {
                let settings = *self.settings.lock().unwrap();
                let state = self.state.lock().unwrap();

                if let Some(ref info) = state.info {
                    let latency = gst::ClockTime::SECOND
                        .mul_div_floor(settings.samples_per_buffer as u64, info.rate() as u64)
                        .unwrap();
                    gst::debug!(CAT, imp = self, "Returning latency {}", latency);
                    q.set(settings.is_live, latency, gst::ClockTime::NONE);
                    true
                } else {
                    false
                }
            }
            _ => BaseSrcImplExt::parent_query(self, query),
        }
    }

    fn is_seekable(&self) -> bool {
        false
    }

    fn unlock(&self) -> Result<(), gst::ErrorMessage> {
        // This should unblock the create() function ASAP, so we
        // just unschedule the clock it here, if any.
        gst::debug!(CAT, imp = self, "Unlocking");
        let mut clock_wait = self.clock_wait.lock().unwrap();
        if let Some(clock_id) = clock_wait.clock_id.take() {
            clock_id.unschedule();
        }
        clock_wait.flushing = true;

        Ok(())
    }

    fn unlock_stop(&self) -> Result<(), gst::ErrorMessage> {
        // This signals that unlocking is done, so we can reset
        // all values again.
        gst::debug!(CAT, imp = self, "Unlock stop");
        let mut clock_wait = self.clock_wait.lock().unwrap();
        clock_wait.flushing = false;

        Ok(())
    }
}

impl PushSrcImpl for BigEyeSrc {
    // Creates the video buffers
    fn create(
        &self,
        _buffer: Option<&mut gst::BufferRef>,
    ) -> Result<CreateSuccess, gst::FlowError> {
        // Get a locked reference to our state, i.e. the input and output VideoInfo
        let mut state = self.state.lock().unwrap();
        let info = match state.info {
            None => {
                gst::element_imp_error!(self, gst::CoreError::Negotiation, ["Have no caps yet"]);
                return Err(gst::FlowError::NotNegotiated);
            }
            Some(ref info) => info.clone(),
        };

        // If a stop position is set (from a seek), only produce samples up to that
        // point but at most samples_per_buffer samples per buffer
        let n_samples = if let Some(sample_stop) = state.sample_stop {
            if sample_stop <= state.sample_offset {
                gst::log!(CAT, imp = self, "At EOS");
                return Err(gst::FlowError::Eos);
            }

            sample_stop - state.sample_offset
        } else {
            settings.samples_per_buffer as u64
        };

        // Allocate a new buffer of the required size, update the metadata with the
        // current timestamp and duration and then fill it according to the current
        // caps
        let mut buffer =
            gst::Buffer::with_size((n_samples as usize) * (info.bpf() as usize)).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();

            // Calculate the current timestamp (PTS) and the next one,
            // and calculate the duration from the difference instead of
            // simply the number of samples to prevent rounding errors
            let pts = state
                .sample_offset
                .mul_div_floor(*gst::ClockTime::SECOND, info.rate() as u64)
                .map(gst::ClockTime::from_nseconds)
                .unwrap();
            let next_pts = (state.sample_offset + n_samples)
                .mul_div_floor(*gst::ClockTime::SECOND, info.rate() as u64)
                .map(gst::ClockTime::from_nseconds)
                .unwrap();
            buffer.set_pts(pts);
            buffer.set_duration(next_pts - pts);

            // Map the buffer writable and create the actual samples
            let mut map = buffer.map_writable().unwrap();
            let data = map.as_mut_slice();

            
            Self::process(
                data,
                &mut state.accumulator,
            );
        }
        state.sample_offset += n_samples;
        drop(state);

        gst::debug!(CAT, imp = self, "Produced buffer {:?}", buffer);

        Ok(CreateSuccess::NewBuffer(buffer))
    }
}