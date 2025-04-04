// Copyright (C) 2025 Roberto Viola <rviola@vicomtech.org>
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at
// <https://mozilla.org/MPL/2.0/>.
//
// SPDX-License-Identifier: MPL-2.0

use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::*;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::io::Write;
use std::fs::File;
use std::path::Path;
use std::collections::HashMap;

const DEFAULT_TARGET_DURATION: u32 = 10;
const DEFAULT_LATENCY: gst::ClockTime =
    gst::ClockTime::from_mseconds((DEFAULT_TARGET_DURATION * 500) as u64);
const DEFAULT_SYNC: bool = true;
const DEFAULT_LOCATION: &str = "manifest.mpd";
const DEFAULT_INIT_LOCATION: &str = "init.cmfi";
const DEFAULT_SEGMENT_LOCATION: &str = "segment_%d.cmfv";

struct DashCmafSinkSettings {
    location: String,
    init_location: String,
	segment_location: String,
    target_duration: u32,
    sync: bool,
	latency: gst::ClockTime,
}

struct DashCmafSinkStream {
    segment_idx: usize,
	start_time: Option<gst::ClockTime>,
    end_time: Option<gst::ClockTime>,
	bandwidth: u64,
    cmafmux: gst::Element,
    appsink: gst_app::AppSink,
}

#[derive(Default)]
pub struct DashCmafSink {
    settings: Mutex<DashCmafSinkSettings>,
	streams: Mutex<HashMap<String, DashCmafSinkStream>>,
}

#[glib::object_subclass]
impl ObjectSubclass for DashCmafSink {
	const NAME: &'static str = "DashCmafSink";
	type Type = super::DashCmafSink;
	type ParentType = gst::Bin;
}

impl Default for DashCmafSinkSettings {
    fn default() -> Self {
        Self {
			location: String::from(DEFAULT_LOCATION),
            init_location: String::from(DEFAULT_INIT_LOCATION),
            segment_location: String::from(DEFAULT_SEGMENT_LOCATION),
            target_duration: DEFAULT_TARGET_DURATION,
            sync: DEFAULT_SYNC,
            latency: DEFAULT_LATENCY,
        }
    }
}

impl Default for DashCmafSinkStream {
    fn default() -> Self {
		let cmafmux = gst::ElementFactory::make("cmafmux")
			.property(
				"fragment-duration",
				gst::ClockTime::from_seconds(DEFAULT_TARGET_DURATION as u64),
			)
			.property("latency", DEFAULT_LATENCY)
			.build()
			.expect("Could not create cmafmux");

		let appsink = gst_app::AppSink::builder()
			.buffer_list(true)
			.sync(DEFAULT_SYNC)
			.build();

        Self {
			segment_idx: 0,
			start_time: Some(gst::ClockTime::from_seconds(0)),
			end_time: Some(gst::ClockTime::from_seconds(0)),
			bandwidth: 0,
			cmafmux,
			appsink,
        }
    }
}

impl BinImpl for DashCmafSink {}

impl ObjectImpl for DashCmafSink {
	fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
            vec![
				glib::ParamSpecString::builder("location")
                    .nick("MPD Location")
                    .blurb("Path to write manifest (MPD)")
                    .default_value(Some(DEFAULT_LOCATION))
                    .build(),
                glib::ParamSpecString::builder("init-location")
                    .nick("Init Segment Location")
                    .blurb("Path to write init segment")
                    .default_value(Some(DEFAULT_INIT_LOCATION))
                    .build(),
				glib::ParamSpecString::builder("segment-location")
                    .nick("Segment Location")
                    .blurb("Template for CMAF segment files")
                    .default_value(Some(DEFAULT_SEGMENT_LOCATION))
                    .build(),
                glib::ParamSpecUInt::builder("target-duration")
                    .nick("Target Duration")
                    .blurb("Target duration in seconds for each segment")
                    .default_value(DEFAULT_TARGET_DURATION)
                    .mutable_ready()
                    .build(),
                glib::ParamSpecBoolean::builder("sync")
                    .nick("Sync")
                    .blurb("Whether to sync appsink to the pipeline clock")
                    .default_value(DEFAULT_SYNC)
                    .build(),
                glib::ParamSpecUInt64::builder("latency")
                    .nick("Latency")
                    .blurb("Latency in nanoseconds")
                    .default_value(DEFAULT_LATENCY.nseconds())
                    .build(),
            ]
        });
        PROPERTIES.as_ref()
    }

	fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
		let mut settings = self.settings.lock().unwrap();
	
		match pspec.name() {
			"location" => {
				settings.location = value
					.get::<Option<String>>()
					.expect("type checked upstream")
					.unwrap_or_else(|| DEFAULT_LOCATION.into());
			}
			"init-location" => {
				settings.init_location = value
					.get::<Option<String>>()
					.expect("type checked upstream")
					.unwrap_or_else(|| DEFAULT_INIT_LOCATION.into());
			}
			"segment-location" => {
				settings.segment_location = value
					.get::<Option<String>>()
					.expect("type checked upstream")
					.unwrap_or_else(|| DEFAULT_SEGMENT_LOCATION.into());
			}
			"target-duration" => {
				settings.target_duration = value.get().expect("type checked upstream");
			}
			"sync" => {
				settings.sync = value.get().expect("type checked upstream");
			}
			"latency" => {
				let latency_ns = value.get::<u64>().expect("type checked upstream");
				settings.latency = gst::ClockTime::from_nseconds(latency_ns);
			}
			_ => unimplemented!(),
		}
	}

	fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
		let settings = self.settings.lock().unwrap();
	
		match pspec.name() {
			"location" => settings.location.to_value(),
			"init-location" => settings.init_location.to_value(),
			"segment-location" => settings.segment_location.to_value(),
			"target-duration" => settings.target_duration.to_value(),
			"sync" => settings.sync.to_value(),
			"latency" => settings.latency.nseconds().to_value(),
			_ => unimplemented!("Property {} not implemented", pspec.name()),
		}
	}

    fn constructed(&self) {
        self.parent_constructed();
    }
}

impl GstObjectImpl for DashCmafSink {}

impl ElementImpl for DashCmafSink {
	fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
		static ELEMENT_METADATA: LazyLock<gst::subclass::ElementMetadata> = LazyLock::new(|| {
			gst::subclass::ElementMetadata::new(
				"DASH CMAF Sink",
				"Sink/Network/Dash",
				"Handles H264/AAC media buffers",
				"Roberto Viola <rviola@vicomtech.org>",
			)
		});
		Some(&*ELEMENT_METADATA)
	}

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            let pad_template = gst::PadTemplate::new(
                "sink_%u",
                gst::PadDirection::Sink,
                gst::PadPresence::Request,
                &[
                    gst::Structure::builder("video/x-h264")
                        .field("stream-format", gst::List::new(["avc", "avc3"]))
                        .field("alignment", "au")
                        .field("width", gst::IntRange::new(1, u16::MAX as i32))
                        .field("height", gst::IntRange::new(1, u16::MAX as i32))
                        .build(),
                    gst::Structure::builder("audio/mpeg")
                        .field("mpegversion", 4i32)
                        .field("stream-format", "raw")
                        .field("channels", gst::IntRange::new(1, u16::MAX as i32))
                        .field("rate", gst::IntRange::new(1, i32::MAX))
                        .build(),
                ]
                .into_iter()
                .collect::<gst::Caps>(),
            )
            .unwrap();

            vec![pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

	fn request_new_pad(
		&self,
		_template: &gst::PadTemplate,
		_name: Option<&str>,
		_caps: Option<&gst::Caps>,
	) -> Option<gst::Pad> {
		let pad_name = _name.map(|s| s.to_string()).unwrap_or_else(|| {
			format!("sink_{}", self.streams.lock().unwrap().len())
		});
	
		gst::info!(CAT, imp = self, "Requesting new pad: {pad_name}");
	
		// Create stream components
		let stream = DashCmafSinkStream::default();
		let settings = self.settings.lock().unwrap();
		let obj = self.obj();

		stream.cmafmux.set_property(
			"fragment-duration",
			gst::ClockTime::from_seconds(settings.target_duration as u64),
		);
		stream.cmafmux.set_property("latency", settings.latency);
		stream.appsink.set_property("sync", settings.sync);
	
		// Add and link elements
		obj.add_many([&stream.cmafmux, stream.appsink.upcast_ref()]).ok()?;
		stream.cmafmux.link(&stream.appsink).ok()?;
	
		// Ghost pad
		let target_pad = stream.cmafmux.static_pad("sink")?;
		// let gpad = gst::GhostPad::with_target(&target_pad).ok()?;
		let gpad = gst::GhostPad::builder(gst::PadDirection::Sink)
			.name(&pad_name) 
			.build();
		gpad.set_target(Some(&target_pad)).expect("Failed to set target pad");
		gpad.set_active(true).ok()?;
		obj.add_pad(&gpad).ok()?;
	
		// Appsink callback
		let stream_pad_name = pad_name.clone();
		let self_weak = self.downgrade();
		stream.appsink.set_callbacks(
			gst_app::AppSinkCallbacks::builder()
				.new_sample(move |sink| {
					let Some(imp) = self_weak.upgrade() else {
						return Err(gst::FlowError::Eos);
					};
	
					let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
					imp.on_new_sample(sample, &stream_pad_name) // you could pass pad_name if needed
				})
				.build(),
		);
	
		// Store the stream context
		let mut streams = self.streams.lock().unwrap();
		streams.insert(pad_name.clone(), stream);
	
		Some(gpad.upcast())
	}

	fn release_pad(&self, _pad: &gst::Pad) {
		let pad_name = _pad.name();
		let mut streams = self.streams.lock().unwrap();
		streams.remove(pad_name.as_str());
	}
}

impl BaseSinkImpl for DashCmafSink {}

impl DashCmafSink {

    fn on_init_segment(&self, pad_name: &str) -> Result<File, std::io::Error> {
        let settings = self.settings.lock().unwrap();
		let location = format!("{}_{}", pad_name, &settings.init_location);
        let path = Path::new(&location);

        File::create(path)
    }

    fn on_new_segment(&self, pad_name: &str) -> Result<(File, String), std::io::Error> {
        let mut streams = self.streams.lock().unwrap();
		let stream = streams.get_mut(pad_name).unwrap(); 
        let settings = self.settings.lock().unwrap();

		let temp_location= sprintf::sprintf!(&settings.segment_location, stream.segment_idx).unwrap();
		let location = format!("{}_{}", pad_name, temp_location);
        stream.segment_idx += 1;
		stream.start_time = Some(gst::ClockTime::from_seconds((0) as u64));
		stream.end_time = Some(gst::ClockTime::from_seconds((settings.target_duration) as u64 * stream.segment_idx as u64));

        let path = Path::new(&location);

        let file = File::create(&path)?;
        Ok((file, location))
    }

    fn add_segment(
        &self,
		_pad_name: &str
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
		let mut streams = self.streams.lock().unwrap();
		let settings = self.settings.lock().unwrap();
		let path = settings.location.clone();

		gst::info!(
			CAT,
			imp = self,
			"writing manifest to {}",
			path
		);

		let mut duration = 0;

		let mut video_reps = Vec::new();
		let mut audio_reps = Vec::new();
		for (pad_name, stream) in streams.iter_mut() {

			duration = stream
				.end_time
				.opt_checked_sub(stream.start_time)
				.ok()
				.flatten()
				.unwrap()
				.mseconds();

			let obj = self.obj();
			let sink_pad = obj.static_pad(pad_name).expect("Missing sink pad");
			let caps = sink_pad.current_caps().unwrap();
			let s = caps.structure(0);

			let (media, codec) = if let Some(s) = s {
				let media_type = s.name();
			
				let (media, codec) = match media_type.as_str() {
					"video/x-h264" => ("video".to_string(), "avc1.64001e".to_string()),
					"audio/mpeg" => ("audio".to_string(), "mp4a.40.2".to_string()),
					_ => ("unknown".to_string(), "unknown".to_string()),
				};
			
				(media, codec)
			} else {
				("unknown".to_string(), "unknown".to_string())
			};

			match media.as_str() {
				"video" => {
					let (width, height, framerate) = if let Some(s) = s {
						let width = s.get::<i32>("width").unwrap_or(1280);
						let height = s.get::<i32>("height").unwrap_or(720);
						let fps = s.get::<gst::Fraction>("framerate").unwrap_or(gst::Fraction::new(30, 1));
						let framerate = format!("{}/{}", fps.numer(), fps.denom());
					
						(width, height, framerate)
					} else {
						(1280, 720, "30/1".to_string())
					};

					gst::info!(
						CAT,
						imp = self,
						"MPD info: media={} codec={} width={} height={} framerate={}",
						media, codec, width, height, framerate
					);

					let segment_location= settings.segment_location.replace ("%d", "$Number$");
					let segment_template = dash_mpd::SegmentTemplate {
						timescale: Some(1000),
						duration: Some(settings.target_duration as f64 * 1000.0),
						startNumber: Some(0),
						initialization: Some(format!("{}_{}", pad_name, &settings.init_location)),
						media: Some(format!("{}_{}", pad_name, &segment_location)),
						..Default::default()
					};

					let rep = dash_mpd::Representation {
						id: Some(pad_name.to_string()),
						codecs: Some(codec),
						width: Some(width as u64),
						height: Some(height as u64),
						frameRate: Some(framerate),
						bandwidth: Some(stream.bandwidth as u64),
						SegmentTemplate: Some(segment_template),
						..Default::default()
					};
					video_reps.push(rep)
				},
				"audio" => {
					gst::info!(
						CAT,
						imp = self,
						"MPD info: media={} codec={}",
						media, codec
					);

					let segment_location= settings.segment_location.replace ("%d", "$Number$");
					let segment_template = dash_mpd::SegmentTemplate {
						timescale: Some(1000),
						duration: Some(settings.target_duration as f64 * 1000.0),
						startNumber: Some(0),
						initialization: Some(format!("{}_{}", pad_name, &settings.init_location)),
						media: Some(format!("{}_{}", pad_name, &segment_location)),
						..Default::default()
					};

					let rep = dash_mpd::Representation {
						id: Some(pad_name.to_string()),
						codecs: Some(codec),
						bandwidth: Some(stream.bandwidth as u64),
						SegmentTemplate: Some(segment_template),
						..Default::default()
					};
					audio_reps.push(rep)
				},
				_ => {}
			};
		}

		let mut adaptations = Vec::new();

		if !video_reps.is_empty() {
			adaptations.push(dash_mpd::AdaptationSet {
				contentType: Some("video".into()),
				mimeType: Some("video/mp4".into()),
				segmentAlignment: Some(true),
				subsegmentStartsWithSAP: Some(1),
				representations: video_reps,
				..Default::default()
			});
		}

		if !audio_reps.is_empty() {
			adaptations.push(dash_mpd::AdaptationSet {
				contentType: Some("audio".into()),
				mimeType: Some("audio/mp4".into()),
				segmentAlignment: Some(true),
				subsegmentStartsWithSAP: Some(1),
				representations: audio_reps,
				..Default::default()
			});
		}

		let period = dash_mpd::Period {
			adaptations: adaptations,
			..Default::default()
		};

		let mpd = dash_mpd::MPD {
			mpdtype: Some("static".to_string()),
			xmlns: Some("urn:mpeg:dash:schema:mpd:2011".to_string()),
			schemaLocation: Some("urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd".to_string()),
			profiles: Some("urn:mpeg:dash:profile:isoff-on-demand:2011".to_string()),
			periods: vec![period],
			mediaPresentationDuration: Some(std::time::Duration::from_millis(duration)),
			minBufferTime: Some(std::time::Duration::from_secs(settings.target_duration as u64)),
			..Default::default()
		};

		use serde::ser::Serialize;

		let mut xml = String::new();
		let mut ser = quick_xml::se::Serializer::new(&mut xml);
		ser.indent(' ', 4);
		mpd.serialize(ser).unwrap();

		let manifest = format!(
			r###"<?xml version="1.0" encoding="UTF-8"?>
{xml}
"###
		);

		std::fs::write(path, manifest).expect("failed to write manifest");
        Ok(gst::FlowSuccess::Ok)
    }

    fn on_new_sample(&self, sample: gst::Sample, pad_name: &str) -> Result<gst::FlowSuccess, gst::FlowError> {
		let mut buffer_list = sample.buffer_list_owned().ok_or(gst::FlowError::Error)?;
		let first = buffer_list.get(0).ok_or(gst::FlowError::Error)?;
	
		// Check for init segment (DISCONT or HEADER flags)
		if first
			.flags()
			.contains(gst::BufferFlags::DISCONT | gst::BufferFlags::HEADER)
		{
			let mut stream = self.on_init_segment(pad_name).map_err(|err| {
				gst::error!(
					CAT,
					imp = self,
					"Couldn't get output stream for init segment: {err}",
				);
				gst::FlowError::Error
			})?;
	
			let map = first.map_readable().map_err(|_| {
				gst::error!(CAT, imp = self, "Failed to map init segment buffer");
				gst::FlowError::Error
			})?;
	
			stream.write_all(&map).map_err(|_| {
				gst::error!(CAT, imp = self, "Couldn't write init segment to output stream");
				gst::FlowError::Error
			})?;
	
			stream.flush().map_err(|_| {
				gst::error!(CAT, imp = self, "Couldn't flush init segment stream");
				gst::FlowError::Error
			})?;
	
			drop(map);
	
			// Remove init segment from buffer list
			buffer_list.make_mut().remove(0..1);
	
			if buffer_list.is_empty() {
				return Ok(gst::FlowSuccess::Ok);
			}
		}
	
		// Get output stream + location
		let (mut stream, _location) = self.on_new_segment(pad_name).map_err(|err| {
			gst::error!(
				CAT,
				imp = self,
				"Couldn't get output stream for fragment: {err}",
			);
			gst::FlowError::Error
		})?;
	
		let mut total_size = 0;
		// Write all fragment buffers
		for buffer in &*buffer_list {
			let map = buffer.map_readable().map_err(|_| {
				gst::error!(CAT, imp = self, "Failed to map fragment buffer");
				gst::FlowError::Error
			})?;
	
			stream.write_all(&map).map_err(|_| {
				gst::error!(CAT, imp = self, "Couldn't write fragment to output stream");
				gst::FlowError::Error
			})?;
			total_size += map.size();
		}
		{
			let mut streams = self.streams.lock().unwrap();
			let dash_stream = streams.get_mut(pad_name).unwrap(); 
			let settings = self.settings.lock().unwrap();
			dash_stream.bandwidth = total_size as u64 * 8 / settings.target_duration as u64;
			gst::info!(CAT, imp = self, "total size: {} bandwidth: {}", total_size, dash_stream.bandwidth);
		};
		
	
		stream.flush().map_err(|_| {
			gst::error!(CAT, imp = self, "Couldn't flush fragment stream");
			gst::FlowError::Error
		})?;
	
		self.add_segment(pad_name)
	}	
}

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "dashcmafsink",
        gst::DebugColorFlags::empty(),
        Some("DASH CMAF Sink"),
    )
});