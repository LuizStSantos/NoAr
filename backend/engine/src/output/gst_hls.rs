//! HLS manifests for MPEG-TS segments finalized by native splitmuxsink.

use super::{scte35::SpliceControl, vtt::VttCue};
use crate::HlsSubtitle;
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use std::{
    collections::VecDeque,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

struct Segment {
    sequence: u64,
    uri: String,
    duration: f64,
    start_ns: u64,
    discontinuity: bool,
}

pub(super) struct HlsPlaylist {
    path: PathBuf,
    stem: String,
    first_sequence: u64,
    segments: VecDeque<Segment>,
    segment_seconds: u32,
    list_size: usize,
    end_ns: u64,
    generation: String,
    first_fragment: bool,
    subtitle: Option<HlsSubtitle>,
    cues: VecDeque<VttCue>,
    program_date_time: bool,
    started_at: SystemTime,
    splice: Option<SpliceControl>,
}

impl HlsPlaylist {
    pub(super) fn new(
        path: &str,
        segment_seconds: u32,
        list_size: u32,
        subtitle: Option<HlsSubtitle>,
        program_date_time: bool,
    ) -> Result<Self> {
        ensure!(
            segment_seconds > 0 && list_size > 0,
            "HLS duration and list size must be positive"
        );
        if let Some(subtitle) = subtitle.as_ref() {
            subtitle.validate().map_err(anyhow::Error::msg)?;
            ensure!(
                !subtitle.name.contains('"') && !subtitle.language.contains('"'),
                "subtitle fields must not contain quotes"
            );
        }
        let path = PathBuf::from(path);
        let parent = path.parent().context("HLS playlist needs a directory")?;
        fs::create_dir_all(parent)?;
        let stem = path
            .file_stem()
            .and_then(|v| v.to_str())
            .context("invalid HLS name")?
            .to_owned();
        let paths = [path.to_string_lossy().to_string()];
        let master = super::hls::master_playlist_path(&paths[0]);
        let fresh = super::hls::prepare_resume_start_number(&paths, master.to_str())?.is_some();
        let mut segments = VecDeque::new();
        if fresh && let Ok(text) = fs::read_to_string(&path) {
            let mut sequence = text
                .lines()
                .find_map(|line| line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:")?.parse().ok())
                .unwrap_or(0);
            let mut duration = None;
            let mut discontinuity = false;
            for line in text.lines() {
                if let Some(value) = line.strip_prefix("#EXTINF:") {
                    duration = value.trim_end_matches(',').parse::<f64>().ok();
                } else if line == "#EXT-X-DISCONTINUITY" {
                    discontinuity = true;
                } else if !line.starts_with('#')
                    && !line.trim().is_empty()
                    && let Some(duration) = duration.take().filter(|v| v.is_finite() && *v > 0.0)
                    && Path::new(line).components().count() == 1
                    && parent.join(line).is_file()
                {
                    segments.push_back(Segment {
                        sequence,
                        uri: line.to_owned(),
                        duration,
                        start_ns: 0,
                        discontinuity,
                    });
                    sequence += 1;
                    discontinuity = false;
                }
            }
        }
        let first_sequence = segments.back().map_or(0, |last| last.sequence + 1);
        let now = SystemTime::now();
        let generation = format!(
            "{}_{}",
            now.duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id()
        );
        Ok(Self {
            path,
            stem,
            first_sequence,
            segments,
            segment_seconds,
            list_size: list_size as usize,
            end_ns: 0,
            generation,
            first_fragment: true,
            subtitle,
            cues: VecDeque::new(),
            program_date_time,
            started_at: now,
            splice: None,
        })
    }

    pub(super) fn segment_path(&self, index: u32) -> String {
        self.path
            .with_file_name(format!(
                "{}_{}_{}.ts",
                self.stem,
                self.generation,
                self.first_sequence + u64::from(index)
            ))
            .to_string_lossy()
            .to_string()
    }

    pub(super) fn add_cue(&mut self, cue: &VttCue) {
        self.cues.push_back(VttCue {
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.text.clone(),
        });
    }

    pub(super) fn set_splice(&mut self, control: SpliceControl) {
        self.started_at = control.hls_origin();
        self.program_date_time = true;
        self.splice = Some(control);
    }

    pub(super) fn closed(&mut self, location: &str, end_ns: u64) -> Result<()> {
        let path = Path::new(location);
        ensure!(
            path.parent() == self.path.parent(),
            "HLS segment escaped the output directory"
        );
        let uri = path
            .file_name()
            .and_then(|v| v.to_str())
            .context("invalid segment filename")?
            .to_owned();
        let sequence = uri
            .trim_end_matches(".ts")
            .rsplit('_')
            .next()
            .context("missing segment sequence")?
            .parse()?;
        let duration = end_ns.saturating_sub(self.end_ns) as f64 / 1_000_000_000.0;
        if duration <= 0.0 {
            return Ok(());
        }
        if self.subtitle.is_some() {
            self.write_vtt(path, self.end_ns, end_ns)?;
        }
        self.segments.push_back(Segment {
            sequence,
            uri,
            duration,
            start_ns: self.end_ns,
            discontinuity: self.first_fragment && self.first_sequence > 0,
        });
        self.first_fragment = false;
        self.end_ns = end_ns;
        let mut obsolete = Vec::new();
        while self.segments.len() > self.list_size {
            if let Some(segment) = self.segments.pop_front() {
                obsolete.push(segment.uri);
            }
        }
        self.publish()?;
        for uri in obsolete {
            let file = self.path.with_file_name(uri);
            if file.parent() == self.path.parent() {
                let _ = fs::remove_file(&file);
                let _ = fs::remove_file(file.with_extension("vtt"));
            }
        }
        let oldest = self
            .segments
            .front()
            .map_or(0, |s| (s.start_ns / 1_000_000) as i64);
        self.cues.retain(|cue| cue.end_ms > oldest);
        Ok(())
    }

    fn header(&self) -> String {
        let target = self
            .segments
            .iter()
            .map(|s| s.duration.ceil() as u32)
            .max()
            .unwrap_or(self.segment_seconds)
            .max(self.segment_seconds);
        let sequence = self
            .segments
            .front()
            .map_or(self.first_sequence, |s| s.sequence);
        format!(
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:{sequence}\n#EXT-X-INDEPENDENT-SEGMENTS\n"
        )
    }

    fn publish(&self) -> Result<()> {
        let mut text = self.header();
        let mut subtitles = self.header();
        let splices = self
            .splice
            .as_ref()
            .map(SpliceControl::hls_splices)
            .unwrap_or_default();
        let oldest = self.segments.front().map_or(0, |s| s.start_ns);
        for splice in &splices {
            if splice.end_ns < oldest {
                continue;
            }
            let start: DateTime<Utc> =
                (self.started_at + std::time::Duration::from_nanos(splice.start_ns)).into();
            let duration = (splice.end_ns - splice.start_ns) as f64 / 1_000_000_000.0;
            let hex = |bytes: &[u8]| {
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02X}"))
                    .collect::<String>()
            };
            text.push_str(&format!(
                "#EXT-X-DATERANGE:ID=\"splice-{}-{}\",START-DATE=\"{}\",PLANNED-DURATION={duration:.6},SCTE35-OUT=0x{}",
                self.started_at.duration_since(UNIX_EPOCH)?.as_nanos(), splice.event_id, start.to_rfc3339(), hex(&splice.out)
            ));
            if let Some(incoming) = &splice.incoming {
                let end: DateTime<Utc> =
                    (self.started_at + std::time::Duration::from_nanos(splice.end_ns)).into();
                text.push_str(&format!(
                    ",END-DATE=\"{}\",DURATION={duration:.6},SCTE35-IN=0x{}",
                    end.to_rfc3339(),
                    hex(incoming)
                ));
            }
            text.push('\n');
        }
        for segment in &self.segments {
            if segment.discontinuity {
                text.push_str("#EXT-X-DISCONTINUITY\n");
                subtitles.push_str("#EXT-X-DISCONTINUITY\n");
            }
            if self.program_date_time {
                let at: DateTime<Utc> =
                    (self.started_at + std::time::Duration::from_nanos(segment.start_ns)).into();
                text.push_str(&format!("#EXT-X-PROGRAM-DATE-TIME:{}\n", at.to_rfc3339()));
            }
            for splice in &splices {
                if segment.start_ns == splice.start_ns {
                    text.push_str(&format!(
                        "#EXT-X-CUE-OUT:{:.6}\n",
                        (splice.end_ns - splice.start_ns) as f64 / 1_000_000_000.0
                    ));
                } else if segment.start_ns > splice.start_ns && segment.start_ns < splice.end_ns {
                    text.push_str(&format!(
                        "#EXT-X-CUE-OUT-CONT:ElapsedTime={:.6},Duration={:.6}\n",
                        (segment.start_ns - splice.start_ns) as f64 / 1_000_000_000.0,
                        (splice.end_ns - splice.start_ns) as f64 / 1_000_000_000.0
                    ));
                } else if segment.start_ns == splice.end_ns {
                    text.push_str("#EXT-X-CUE-IN\n");
                }
            }
            text.push_str(&format!(
                "#EXTINF:{:.6},\n{}\n",
                segment.duration, segment.uri
            ));
            subtitles.push_str(&format!(
                "#EXTINF:{:.6},\n{}\n",
                segment.duration,
                Path::new(&segment.uri)
                    .with_extension("vtt")
                    .to_string_lossy()
            ));
        }
        atomic_write(&self.path, &text)?;
        if self.subtitle.is_some() {
            atomic_write(
                &self.path.with_file_name(format!("{}_vtt.m3u8", self.stem)),
                &subtitles,
            )?;
        }
        Ok(())
    }

    fn write_vtt(&self, path: &Path, start_ns: u64, end_ns: u64) -> Result<()> {
        let start = (start_ns / 1_000_000) as i64;
        let end = (end_ns / 1_000_000) as i64;
        let pts = first_video_pts(path)?.unwrap_or(start.saturating_mul(90));
        let mut text = format!("WEBVTT\nX-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:{pts}\n\n");
        for cue in self
            .cues
            .iter()
            .filter(|cue| cue.end_ms > start && cue.start_ms < end)
        {
            text.push_str(&format!(
                "{} --> {}\n{}\n\n",
                timestamp((cue.start_ms - start).max(0)),
                timestamp((cue.end_ms.min(end) - start).max(0)),
                cue.text
            ));
        }
        atomic_write(&path.with_extension("vtt"), &text)
    }

    pub(super) fn write_master(
        path: &str,
        variants: &[(String, u32, u32, u64)],
        subtitle: Option<&HlsSubtitle>,
    ) -> Result<()> {
        let path = Path::new(path);
        let master = path.with_file_name("master.m3u8");
        if variants.is_empty() && subtitle.is_none() {
            let _ = fs::remove_file(master);
            return Ok(());
        }
        let mut text = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
        let name = path
            .file_stem()
            .and_then(|v| v.to_str())
            .context("invalid HLS name")?;
        if let Some(subtitle) = subtitle {
            text.push_str(&format!("#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",NAME=\"{}\",LANGUAGE=\"{}\",DEFAULT={},AUTOSELECT=YES,URI=\"{}_vtt.m3u8\"\n", subtitle.name, subtitle.language, if subtitle.default { "YES" } else { "NO" }, name));
        }
        let group = if subtitle.is_some() {
            ",SUBTITLES=\"subs\""
        } else {
            ""
        };
        if variants.is_empty() {
            text.push_str(&format!(
                "#EXT-X-STREAM-INF:BANDWIDTH=4000000{group}\n{name}.m3u8\n"
            ));
        }
        for (name, width, height, bitrate) in variants {
            text.push_str(&format!("#EXT-X-STREAM-INF:BANDWIDTH={bitrate},RESOLUTION={width}x{height}{group}\n{name}.m3u8\n"));
        }
        atomic_write(&master, &text)
    }
}

fn timestamp(ms: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn atomic_write(path: &Path, text: &str) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn first_video_pts(path: &Path) -> Result<Option<i64>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(128 * 1024)
        .read_to_end(&mut bytes)?;
    for packet in bytes.as_chunks::<188>().0 {
        if packet[0] != 0x47 || packet[1] & 0x40 == 0 {
            continue;
        }
        let mut offset = 4;
        if packet[3] & 0x20 != 0 {
            offset += 1 + usize::from(packet[4]);
        }
        if offset + 14 > 188 {
            continue;
        }
        let pes = &packet[offset..];
        if pes[..3] != [0, 0, 1] || !(0xe0..=0xef).contains(&pes[3]) || pes[7] & 0x80 == 0 {
            continue;
        }
        let p = &pes[9..14];
        return Ok(Some(
            (i64::from((p[0] >> 1) & 7) << 30)
                | (i64::from(p[1]) << 22)
                | (i64::from(p[2] >> 1) << 15)
                | (i64::from(p[3]) << 7)
                | i64::from(p[4] >> 1),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scte35Config;

    #[test]
    fn dateranges_share_the_media_clock_and_gain_an_actual_end_only_after_the_splice() {
        let directory = std::env::temp_dir().join(format!(
            "noar-hls-cues-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("stream.m3u8");
        let control = SpliceControl::default();
        control.open_output(
            Scte35Config {
                pid: 500,
                preroll_seconds: 2.0,
            },
            25,
            (1, 25),
        );
        control.frame_needs_key_frame(0);
        control.insert_break(6.0).unwrap();
        control.video_packet_muxed(3600);
        control.messages_due(324_003_600, true);
        let start = control.hls_splices()[0].start_ns;
        let end = control.hls_splices()[0].end_ns;
        let mut playlist = HlsPlaylist::new(path.to_str().unwrap(), 2, 4, None, false).unwrap();
        playlist.set_splice(control.clone());
        let close = |playlist: &mut HlsPlaylist, index, at| {
            let location = playlist.segment_path(index);
            fs::write(&location, []).unwrap();
            playlist.closed(&location, at).unwrap();
        };
        close(&mut playlist, 0, start);
        let announcement = fs::read_to_string(&path).unwrap();
        assert!(announcement.contains("#EXT-X-DATERANGE:"));
        assert!(announcement.contains("SCTE35-OUT=0xFC"));
        assert!(announcement.contains("#EXT-X-PROGRAM-DATE-TIME:"));
        assert!(!announcement.contains("SCTE35-IN="));
        assert!(!announcement.contains(",DURATION="));
        close(&mut playlist, 1, start + 2_000_000_000);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("#EXT-X-CUE-OUT:6.000000")
        );
        for frame in 2..=202 {
            control.video_packet_muxed(frame * 3600);
            control.frame_needs_key_frame(frame);
            control.messages_due(324_000_000 + frame * 3600, true);
        }
        close(&mut playlist, 2, end);
        close(&mut playlist, 3, end + 2_000_000_000);
        let returned = fs::read_to_string(&path).unwrap();
        assert!(returned.contains("SCTE35-IN=0xFC"));
        assert!(returned.contains(",DURATION=6.000000"));
        assert!(returned.contains("#EXT-X-CUE-IN\n"));
        assert!(returned.contains("#EXT-X-CUE-OUT-CONT:"));
        fs::remove_dir_all(directory).unwrap();
    }
}
