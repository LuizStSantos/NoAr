//! DVB service metadata sent to the native MPEG-TS muxer.

use ::gstreamer::{
    self as gst,
    glib::translate::{FromGlibPtrFull, ToGlibPtr, ToGlibPtrMut},
};
use anyhow::{Result, ensure};
use gstreamer_mpegts::{self as mpegts, ffi};
use std::{collections::BTreeMap, ffi::CString};

pub(super) fn set_service(mux: &gst::Element, tags: &BTreeMap<String, String>) -> Result<()> {
    for key in tags.keys() {
        ensure!(
            matches!(key.as_str(), "service_name" | "service_provider"),
            "MPEG-TS metadata {key:?} has no DVB service mapping"
        );
    }
    let name = CString::new(tags.get("service_name").map_or("NoAr", String::as_str))?;
    let provider = CString::new(tags.get("service_provider").map_or("NoAr", String::as_str))?;
    ensure!(
        name.as_bytes().len() + provider.as_bytes().len() <= 240,
        "DVB service metadata is too long"
    );
    // SAFETY: native constructors allocate initialized, owned pointer arrays.
    // Descriptor/service ownership transfers into those arrays, then the table
    // transfers to the GLib boxed Section wrapper. send_event retains its own
    // reference while the wrapper remains alive for the call.
    unsafe {
        ffi::gst_mpegts_initialize();
        let descriptor = ffi::gst_mpegts_descriptor_from_dvb_service(
            ffi::GST_DVB_SERVICE_DIGITAL_TELEVISION,
            name.as_ptr(),
            provider.as_ptr(),
        );
        ensure!(
            !descriptor.is_null(),
            "GStreamer cannot encode DVB service metadata"
        );
        let table = ffi::gst_mpegts_sdt_new();
        (*table).actual_ts = 1;
        (*table).transport_stream_id = 1;
        (*table).original_network_id = 1;
        let service = ffi::gst_mpegts_sdt_service_new();
        (*service).service_id = 1;
        (*service).running_status = ffi::GST_MPEGTS_RUNNING_STATUS_RUNNING;
        gst::glib::ffi::g_ptr_array_add((*service).descriptors, descriptor.cast());
        gst::glib::ffi::g_ptr_array_add((*table).services, service.cast());
        let mut section = mpegts::Section::from_glib_full(ffi::gst_mpegts_section_from_sdt(table));
        ensure!(
            ffi::gst_mpegts_section_send_event(section.to_glib_none_mut().0, mux.to_glib_none().0)
                != 0,
            "GStreamer muxer rejected DVB service metadata"
        );
    }
    Ok(())
}
