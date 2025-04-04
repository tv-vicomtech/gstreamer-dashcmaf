use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::subclass::prelude::*;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::io::Write;
use std::fs::File;
use std::path::{Path, PathBuf};

const DEFAULT_TARGET_DURATION: u32 = 2;
const DEFAULT_LATENCY: gst::ClockTime =
    gst::ClockTime::from_mseconds((DEFAULT_TARGET_DURATION * 500) as u64);
const DEFAULT_SYNC: bool = false;
const DEFAULT_INIT_LOCATION: &str = "init.cmfi";
const DEFAULT_CMAF_LOCATION: &str = "segment_%d.cmfv";

struct DashCmafSinkSettings {
    init_location: String,
    location: String,
    target_duration: u32,
    sync: bool,
	latency: gst::ClockTime,
    playlist_root_init: Option<String>,

    cmafmux: gst::Element,
    appsink: gst_app::AppSink,
}

#[derive(Default)]
struct DashCmafSinkState {
    segment_idx: usize,
	start_time: Option<gst::ClockTime>,
    end_time: Option<gst::ClockTime>,
    path: PathBuf,
}

#[derive(Default)]
pub struct DashCmafSink {
    settings: Mutex<DashCmafSinkSettings>,
	state: Mutex<DashCmafSinkState>,
}

#[glib::object_subclass]
impl ObjectSubclass for DashCmafSink {
	const NAME: &'static str = "DashCmafSink";
	type Type = super::DashCmafSink;
	type ParentType = gst::Bin;
}

impl Default for DashCmafSinkSettings {
    fn default() -> Self {
        let cmafmux = gst::ElementFactory::make("cmafmux")
            .name("muxer")
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
            .name("appsink")
            .build();

        Self {
            init_location: String::from(DEFAULT_INIT_LOCATION),
            location: String::from(DEFAULT_CMAF_LOCATION),
            target_duration: DEFAULT_TARGET_DURATION,
            sync: DEFAULT_SYNC,
            latency: DEFAULT_LATENCY,
            playlist_root_init: None,
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
                glib::ParamSpecString::builder("init-location")
                    .nick("Init Segment Location")
                    .blurb("Path to write init.mp4 segment")
                    .default_value(Some(DEFAULT_INIT_LOCATION))
                    .build(),
                glib::ParamSpecString::builder("location")
                    .nick("Segment Location")
                    .blurb("Template for CMAF segment files")
                    .default_value(Some(DEFAULT_CMAF_LOCATION))
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
                glib::ParamSpecString::builder("playlist-root-init")
                    .nick("Playlist Root for Init Segment")
                    .blurb("Optional base URL for init segment in playlist")
                    .build(),
            ]
        });
        PROPERTIES.as_ref()
    }

	fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
		let mut settings = self.settings.lock().unwrap();
	
		match pspec.name() {
			"init-location" => {
				settings.init_location = value
					.get::<Option<String>>()
					.expect("type checked upstream")
					.unwrap_or_else(|| DEFAULT_INIT_LOCATION.into());
			}
			"location" => {
				settings.location = value
					.get::<Option<String>>()
					.expect("type checked upstream")
					.unwrap_or_else(|| DEFAULT_CMAF_LOCATION.into());
			}
			"target-duration" => {
				settings.target_duration = value.get().expect("type checked upstream");
				settings.cmafmux.set_property(
					"fragment-duration",
					gst::ClockTime::from_seconds(settings.target_duration as u64),
				);
			}
			"sync" => {
				settings.sync = value.get().expect("type checked upstream");
				settings.appsink.set_property("sync", settings.sync);
			}
			"latency" => {
				let latency_ns = value.get::<u64>().expect("type checked upstream");
				settings.latency = gst::ClockTime::from_nseconds(latency_ns);
				settings
					.cmafmux
					.set_property("latency", settings.latency);
			}
			"playlist-root-init" => {
				settings.playlist_root_init = value
					.get::<Option<String>>()
					.expect("type checked upstream");
			}
			_ => unimplemented!(),
		}
	}

	fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
		let settings = self.settings.lock().unwrap();
	
		match pspec.name() {
			"init-location" => settings.init_location.to_value(),
			"location" => settings.location.to_value(),
			"target-duration" => settings.target_duration.to_value(),
			"sync" => settings.sync.to_value(),
			"latency" => settings.latency.nseconds().to_value(),
			"playlist-root-init" => settings.playlist_root_init.to_value(),
			_ => unimplemented!("Property {} not implemented", pspec.name()),
		}
	}

    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        let settings = self.settings.lock().unwrap();

        // Add internal elements to this bin
		obj.add_many([&settings.cmafmux, settings.appsink.upcast_ref()])
            .unwrap();

        // Link cmafmux -> appsink
        settings.cmafmux.link(&settings.appsink).unwrap();

        // Create and add ghost pad pointing to cmafmux's sink pad
        let sinkpad = settings.cmafmux.static_pad("sink").unwrap();
        let gpad = gst::GhostPad::with_target(&sinkpad).unwrap();
        gpad.set_active(true).unwrap();
        obj.add_pad(&gpad).unwrap();

        // Set up appsink callback
        let self_weak = self.downgrade();
        settings.appsink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let Some(imp) = self_weak.upgrade() else {
                        return Err(gst::FlowError::Eos);
                    };

                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    imp.on_new_sample(sample)
                })
                .build(),
        );
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
				"Name <name@gmail.com>",
			)
		});
		Some(&*ELEMENT_METADATA)
	}

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: LazyLock<Vec<gst::PadTemplate>> = LazyLock::new(|| {
            let pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
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
}

impl BaseSinkImpl for DashCmafSink {}

impl DashCmafSink {

    fn on_init_segment(&self) -> Result<File, std::io::Error> {
        let settings = self.settings.lock().unwrap();
        let path = Path::new(&settings.init_location);

        File::create(path)
    }

    fn on_new_fragment(&self) -> Result<(File, String), std::io::Error> {
        let mut state = self.state.lock().unwrap();
        let settings = self.settings.lock().unwrap();

        // let location = format!( "{}_{}.mp4", &settings.location, state.segment_idx );
		let location= sprintf::sprintf!(&settings.location, state.segment_idx).unwrap();
        state.segment_idx += 1;
		state.start_time = Some(gst::ClockTime::from_mseconds((0) as u64));
		state.end_time = Some(gst::ClockTime::from_mseconds((2000) as u64 * state.segment_idx as u64));

        let path = Path::new(&location);

        let file = File::create(&path)?;
        Ok((file, location))
    }

    fn add_segment(
        &self,
        // (you could pass duration, time, location, etc. here if needed)
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
		// Now write the manifest
		let state = self.state.lock().unwrap();
		let settings = self.settings.lock().unwrap();
		let mut path = state.path.clone();
		path.push("manifest.mpd");

		println!("writing manifest to {}", path.display());

		let duration = state
			.end_time
			.opt_checked_sub(state.start_time)
			.ok()
			.flatten()
			.unwrap()
			.mseconds();

		let obj = self.obj();
		let sink_pad = obj.static_pad("sink").expect("Missing sink pad");
		let caps = sink_pad.current_caps();

		let (media, codec) = if let Some(ref caps) = caps {
			let s = caps.structure(0).unwrap();
			let media_type = s.name().as_str();
		
			match media_type {
				"video/x-h264" => ("video".to_string(),"avc1.64001e".to_string()),
				"audio/mpeg" => ("audio".to_string(),"mp4a.40.2".to_string()),
				_ => ("unknown".to_string(),"unknown".to_string())
			}
		} else {
			("unknown".to_string(),"unknown".to_string())
		};

		let (width,height, framerate) = if let Some(caps) = caps {
			let s = caps.structure(0).unwrap();
			let width = s.get::<i32>("width").ok().unwrap_or(1280);
			let height = s.get::<i32>("height").ok().unwrap_or(720);
			let framerate = s.get::<i32>("framerate").ok().unwrap_or(30);
			(width, height, framerate)
		} else {
			(1280, 720, 30)
		};

		let segment_location= settings.location.replace ("%d", "$Number$");
		let segment_template = dash_mpd::SegmentTemplate {
			timescale: Some(1000),
			duration: Some(settings.target_duration as f64 * 1000.0),
			startNumber: Some(0),
			initialization: Some(DEFAULT_INIT_LOCATION.to_string()),
			media: Some(segment_location),
			..Default::default()
		};

		let rep = dash_mpd::Representation {
			id: Some("A".to_string()),
			width: Some(width as u64),
			height: Some(height as u64),
			bandwidth: Some(2048000),
			SegmentTemplate: Some(segment_template),
			..Default::default()
		};

		let adapt = dash_mpd::AdaptationSet {
			contentType: Some(media.clone()),
			mimeType: Some(media + "/mp4"),
			codecs: Some(codec),
			frameRate: Some(framerate.to_string() + "/1"),
			segmentAlignment: Some(true),
			subsegmentStartsWithSAP: Some(1),
			representations: vec![rep],
			..Default::default()
		};

		let period = dash_mpd::Period {
			adaptations: vec![adapt],
			..Default::default()
		};

		let mpd = dash_mpd::MPD {
			mpdtype: Some("static".to_string()),
			xmlns: Some("urn:mpeg:dash:schema:mpd:2011".to_string()),
			schemaLocation: Some("urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd".to_string()),
			profiles: Some("urn:mpeg:dash:profile:isoff-on-demand:2011".to_string()),
			periods: vec![period],
			mediaPresentationDuration: Some(std::time::Duration::from_millis(duration)),
			minBufferTime: Some(std::time::Duration::from_secs(1)),
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

    fn on_new_sample(&self, sample: gst::Sample) -> Result<gst::FlowSuccess, gst::FlowError> {
		let mut buffer_list = sample.buffer_list_owned().ok_or(gst::FlowError::Error)?;
		let first = buffer_list.get(0).ok_or(gst::FlowError::Error)?;
	
		// Check for init segment (DISCONT or HEADER flags)
		if first
			.flags()
			.contains(gst::BufferFlags::DISCONT | gst::BufferFlags::HEADER)
		{
			let mut stream = self.on_init_segment().map_err(|err| {
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
		let (mut stream, _location) = self.on_new_fragment().map_err(|err| {
			gst::error!(
				CAT,
				imp = self,
				"Couldn't get output stream for fragment: {err}",
			);
			gst::FlowError::Error
		})?;
	
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
		}
	
		stream.flush().map_err(|_| {
			gst::error!(CAT, imp = self, "Couldn't flush fragment stream");
			gst::FlowError::Error
		})?;
	
		// Notify the playlist index (or whatever your segment tracker is)
		self.add_segment()
	}	
}

static CAT: LazyLock<gst::DebugCategory> = LazyLock::new(|| {
    gst::DebugCategory::new(
        "dashcmafsink",
        gst::DebugColorFlags::empty(),
        Some("DASH CMAF Sink"),
    )
});