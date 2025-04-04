use gst::glib;
use gst::prelude::*;

mod imp;

glib::wrapper! {
    pub struct DashCmafSink(ObjectSubclass<imp::DashCmafSink>) @extends gst::Bin, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
	gst::Element::register(Some(plugin), "dashcmafsink", gst::Rank::NONE, DashCmafSink::static_type())
}