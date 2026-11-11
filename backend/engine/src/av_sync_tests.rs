//! Whether the sound of an event airs with its picture.
//!
//! A reference clip has a white frame and a 40 ms beep that start together
//! once a second. It plays through the real playout and encoder into an
//! MPEG-TS file, across clip changes and a seek. An independent decoder
//! (FFmpeg) then finds every flash and every beep in that file. Every beep
//! must start within EBU R37 of its flash (sound at most [`EARLY`] ahead or
//! [`LATE`] behind), and by the same amount for every event ([`SPREAD`]), so
//! a clip change, a seek or a drift that moves the sound fails.
//!
//! Measured on the MPEG-TS output: the sound starts 21.3 ms late, one AAC
//! frame: the encoder's priming samples are not taken off the audio
//! timestamps, as FFmpeg's own MPEG-TS output does.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{ClipResult, LogoFade, OutputConfig, Playout, StreamType};

const FPS: u32 = 25;
const RATE: usize = 48_000;
/// Frame of each second with the flash, away from the clip boundaries.
const FLASH_FRAME: u32 = 10;
/// How far the sound may lead its picture (EBU R37).
const EARLY: f64 = 0.040;
/// How far the sound may follow its picture (EBU R37).
const LATE: f64 = 0.060;
/// How much the offset may differ between events.
const SPREAD: f64 = 0.002;

fn run(command: &mut Command) -> Vec<u8> {
    let output = command.output().expect("FFmpeg is installed");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Six seconds of black with a white frame and a 1 kHz beep, both starting
/// at frame `FLASH_FRAME` of every second.
fn reference_clip(directory: &Path) -> PathBuf {
    let path = directory.join("flash-beep.mp4");
    let flash = format!(
        "color=c=black:s=320x180:r={FPS}:d=6,format=yuv420p,\
         drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='eq(mod(n\\,{FPS})\\,{FLASH_FRAME})'"
    );
    let start = RATE / FPS as usize * FLASH_FRAME as usize;
    let length = RATE / FPS as usize;
    let beep = format!(
        "aevalsrc='if(gte(mod(n\\,{RATE})\\,{start})*lt(mod(n\\,{RATE})\\,{end})\\,0.5*sin(2*PI*1000*t)\\,0)':s={RATE}:d=6:c=stereo",
        end = start + length
    );
    run(Command::new("ffmpeg")
        .args([
            "-v", "error", "-y", "-f", "lavfi", "-i", &flash, "-f", "lavfi", "-i", &beep,
        ])
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-g",
            "25",
            "-pix_fmt",
            "yuv420p",
        ])
        .args(["-c:a", "aac", "-b:a", "128k", "-shortest"])
        .arg(&path));
    path
}

/// When each white frame of `path` starts, in its own timestamps.
fn flashes(path: &Path) -> Vec<f64> {
    let source = format!("movie={},signalstats", path.display());
    let text = run(Command::new("ffprobe").args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        &source,
        "-show_entries",
        "frame=pts_time:frame_tags=lavfi.signalstats.YAVG",
        "-of",
        "csv=p=0",
    ]));
    String::from_utf8(text)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let (time, luma) = line.split_once(',')?;
            (luma.parse::<f64>().ok()? > 128.0).then(|| time.parse().ok())?
        })
        .collect()
}

/// When each beep of `path` starts, in the same timestamps: the audio is
/// decoded from the start of the file with any gap filled, so a sample's
/// index is its time.
fn beeps(path: &Path) -> Vec<f64> {
    let start: f64 = String::from_utf8(run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=start_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path)))
    .unwrap()
    .trim()
    .parse()
    .unwrap();
    let raw = run(Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "aresample=async=1:first_pts=0,pan=mono|c0=c0",
        ])
        .args(["-ar", "48000", "-f", "f32le", "-"]));
    let mut onsets = vec![];
    // Quiet before a beep: a tenth of a second.
    let mut quiet = RATE / 10;

    for (index, sample) in raw.as_chunks::<4>().0.iter().enumerate() {
        let sample = f32::from_le_bytes(*sample);
        if sample.abs() > 0.1 {
            if quiet >= RATE / 10 {
                onsets.push(start + index as f64 / RATE as f64);
            }
            quiet = 0;
        } else {
            quiet += 1;
        }
    }

    onsets
}

#[test]
fn every_beep_airs_with_its_flash_across_clip_changes_and_a_seek() {
    let directory = std::env::temp_dir().join(format!("noar-av-sync-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let clip = reference_clip(&directory);
    let clip = clip.to_str().unwrap();
    let output = directory.join("output.ts");
    let config = OutputConfig::new(320, 180, FPS, RATE as u32).with_stream_type(StreamType::Udp);
    let mut playout = Playout::open_stream(output.to_str().unwrap(), config, 1.0).unwrap();

    // Three seconds from the start, three from 1.2 s, then two from the start.
    for (seek, length) in [(None, 3.0), (Some(1.2), 3.0), (None, 2.0)] {
        let result = playout
            .play_timed(
                clip,
                seek,
                Some(length),
                None,
                None,
                LogoFade::default(),
                1.0,
            )
            .unwrap();
        assert_eq!(result, ClipResult::Played);
    }
    playout.finish().unwrap();

    let flashes = flashes(&output);
    let beeps = beeps(&output);

    // Each clip airs its flashes: 0.4, 1.4, 2.4; 3.2, 4.2, 5.2; 6.4, 7.4.
    let first = flashes[0];
    let relative = flashes.iter().map(|time| time - first).collect::<Vec<_>>();
    let expected = [0.0, 1.0, 2.0, 2.8, 3.8, 4.8, 6.0, 7.0];
    assert_eq!(relative.len(), expected.len(), "flashes at {flashes:?}");
    for (time, expected) in relative.iter().zip(expected) {
        assert!((time - expected).abs() < 0.001, "flashes at {flashes:?}");
    }

    assert_eq!(
        beeps.len(),
        flashes.len(),
        "beeps at {beeps:?}, flashes at {flashes:?}"
    );
    let offsets = flashes
        .iter()
        .zip(&beeps)
        .map(|(flash, beep)| beep - flash)
        .collect::<Vec<_>>();
    println!("sound minus picture, in seconds: {offsets:?}");
    assert!(
        offsets
            .iter()
            .all(|offset| (-EARLY..=LATE).contains(offset)),
        "sound minus picture, in seconds: {offsets:?}"
    );
    let (lowest, highest) = offsets
        .iter()
        .fold((f64::MAX, f64::MIN), |(low, high), offset| {
            (low.min(*offset), high.max(*offset))
        });
    assert!(
        highest - lowest <= SPREAD,
        "the offset changed between events: {offsets:?}"
    );

    std::fs::remove_dir_all(&directory).ok();
}
