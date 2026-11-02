use anyhow::{Context, Result};
use ffmpeg_next::{ffi, frame};

pub(crate) fn reference_audio_frame(source: &frame::Audio) -> Result<frame::Audio> {
    let mut referenced = frame::Audio::empty();
    let result = unsafe { ffi::av_frame_ref(referenced.as_mut_ptr(), source.as_ptr()) };

    if result < 0 {
        return Err(ffmpeg_next::Error::from(result)).context("referencing FFmpeg audio frame");
    }

    Ok(referenced)
}

pub(crate) fn make_audio_frame_writable(frame: &mut frame::Audio) -> Result<()> {
    let result = unsafe { ffi::av_frame_make_writable(frame.as_mut_ptr()) };

    if result < 0 {
        return Err(ffmpeg_next::Error::from(result)).context("making FFmpeg audio frame writable");
    }

    Ok(())
}
