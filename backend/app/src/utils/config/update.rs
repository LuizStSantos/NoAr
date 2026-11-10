//! Apply a channel configuration change independently of HTTP extraction.
//!
//! Authorization belongs to the route. This operation retains the existing order:
//! validate assets and encoders, save through one DB transaction, then reconcile
//! mail/audio/graphics controls and report whether the running output needs restart.

use std::{path::Path, sync::Arc};

use sqlx::sqlite::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    db::handles,
    file::norm_abs_path,
    player::controller::ChannelManager,
    utils::{errors::ServiceError, mail::MailQueue},
};

use super::{OutputMode, PlayoutConfig, get_config};

type MailQueues = Mutex<Vec<Arc<Mutex<MailQueue>>>>;

pub(crate) async fn apply_config_update(
    pool: &SqlitePool,
    manager: &ChannelManager,
    mail_queues: &MailQueues,
    id: i32,
    mut data: PlayoutConfig,
) -> Result<bool, ServiceError> {
    let _graphics_edit = manager.graphics_takes.begin_edit().await;
    let storage_root = manager.channel.lock().await.storage.clone();
    let storage = Path::new(&storage_root);
    let config = manager.config.read().await.clone();
    let config_id = config.general.id;

    // The remote-control keys change only with `/keys/{kind}`: a page that
    // saves an older copy of the settings must not bring back an old key.
    data.processing.logo_key = config.processing.logo_key.clone();
    data.processing.live_key = config.processing.live_key.clone();
    data.scte35.key = config.scte35.key.clone();

    let (logo_path, _, logo) = norm_abs_path(storage, &data.processing.logo)?;
    let (_, _, filler) = norm_abs_path(storage, &data.storage.filler)?;

    // The logo goes on air when saved, so an image that cannot be read is
    // refused here rather than leaving the old one on air.
    let logo_changed = data.processing.logo != config.processing.logo
        || data.processing.logo_scale != config.processing.logo_scale
        || data.processing.logo_position != config.processing.logo_position
        || data.processing.logo_opacity != config.processing.logo_opacity
        || (data.processing.add_logo && !config.processing.add_logo);

    if logo_changed && data.processing.add_logo && !data.processing.logo.trim().is_empty() {
        let mut check = data.clone();
        check.processing.logo_path = logo_path.to_string_lossy().to_string();
        if let Some(config) = crate::player::output::channel_logo(&check) {
            noar_engine::LogoControl::default()
                .set_configured(Some(&config), data.output.width, data.output.height)
                .map_err(|error| {
                    ServiceError::BadRequest(format!(
                        "{}: the logo cannot be used ({})",
                        data.processing.logo,
                        error.root_cause()
                    ))
                })?;
        }
    }
    data.processing.logo = logo;
    data.storage.filler = filler;
    if !data.processing.lshape.trim().is_empty() {
        let (_, _, lshape) = norm_abs_path(storage, &data.processing.lshape)?;
        data.processing.lshape = lshape;
    }
    // Checked now, so a graphic that cannot be read is refused here rather
    // than leaving the L-shape off on air.
    if let Some(lshape) = data
        .processing
        .lshape_config(storage)
        .map_err(ServiceError::BadRequest)?
    {
        noar_engine::LShapeControl::default()
            .set_graphic(Some(&lshape), data.output.width, data.output.height)
            .map_err(|error| {
                // The cause only: the full path is the server's business.
                ServiceError::BadRequest(format!(
                    "{}: the L-shape graphic cannot be read ({})",
                    data.processing.lshape,
                    error.root_cause()
                ))
            })?;
    }
    data.processing
        .check_graphics_scenes()
        .map_err(ServiceError::BadRequest)?;
    for scene in &mut data.processing.graphics_scenes {
        scene.name = scene.name.trim().to_string();
        for image in [&mut scene.logo_image, &mut scene.lshape_graphic] {
            if let Some(path) = image
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
            {
                let (_, _, relative) = norm_abs_path(storage, path)?;
                *image = Some(relative);
            } else {
                *image = None;
            }
        }
    }
    data.processing
        .hls_subtitle()
        .map_err(ServiceError::BadRequest)?;
    let logo_transitions = data
        .processing
        .logo_transitions()
        .map_err(ServiceError::BadRequest)?;
    noar_engine::AudioEffectsControl::new(data.audio.volume)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    validate_loudness(&data.audio)?;
    data.output.validate().map_err(ServiceError::BadRequest)?;
    data.scte35.validate().map_err(ServiceError::BadRequest)?;
    data.recording
        .validate()
        .map_err(ServiceError::BadRequest)?;
    if data.recording.enable {
        if data.recording.source != crate::utils::config::RecordingSource::Encode
            && data.recording.source_output_id != Some(data.output.id)
        {
            return Err(ServiceError::BadRequest(
                "recording source must reference the active output".to_string(),
            ));
        }
        match data.recording.source {
            crate::utils::config::RecordingSource::Stream
                if data.output.mode != OutputMode::Stream =>
            {
                return Err(ServiceError::BadRequest(
                    "recording stream source must reference a stream output".to_string(),
                ));
            }
            crate::utils::config::RecordingSource::HlsVariant
                if data.output.mode != OutputMode::HLS =>
            {
                return Err(ServiceError::BadRequest(
                    "recording HLS source must reference an HLS output".to_string(),
                ));
            }
            _ => {}
        }
        if data.recording.source == crate::utils::config::RecordingSource::HlsVariant {
            let has_variant = data
                .output
                .hls_streams()
                .map_err(ServiceError::BadRequest)?
                .iter()
                .any(|variant| variant.name == data.recording.variant);
            if !has_variant {
                return Err(ServiceError::BadRequest(format!(
                    "unknown recording HLS variant {:?}",
                    data.recording.variant
                )));
            }
        }
    }

    let is_hls = data.output.mode == OutputMode::HLS;
    let is_encoded = matches!(
        data.output.mode,
        OutputMode::HLS | OutputMode::Stream | OutputMode::Ndi | OutputMode::Sdi
    );
    let video_options = serde_json::to_string(&data.output.video_options)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    let protocol_options = serde_json::to_string(&data.output.protocol_options)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    let muxer_options = serde_json::to_string(&data.output.muxer_options)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    let metadata_options = serde_json::to_string(&data.output.metadata_options)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    let audio_options = serde_json::to_string(&data.output.audio_options)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    // Save the output and channel settings atomically.
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;

    handles::update_output_on(
        &mut transaction,
        data.output.id,
        id,
        &data.output.hls_variants.join(";"),
        &data.output.stream_url,
        (data.output.mode == OutputMode::Stream).then_some(match data.output.stream_type {
            crate::utils::config::StreamType::Rtmp => "rtmp",
            crate::utils::config::StreamType::Srt => "srt",
            crate::utils::config::StreamType::Udp => "udp",
            crate::utils::config::StreamType::Custom => "custom",
        }),
        (data.output.mode == OutputMode::Stream
            && data.output.stream_type == crate::utils::config::StreamType::Custom)
            .then_some(data.output.stream_format.as_str()),
        is_hls.then_some(data.output.hls_playlist_name.as_str()),
        is_hls.then_some(i64::from(data.output.hls_segment_duration)),
        is_hls.then_some(i64::from(data.output.hls_list_size)),
        data.output.desktop_fullscreen,
        i64::from(data.output.width),
        i64::from(data.output.height),
        data.output.fps,
        is_encoded.then_some(data.output.video_codec.as_str()),
        if is_encoded {
            video_options.as_str()
        } else {
            "{}"
        },
        if data.output.mode == OutputMode::Stream {
            protocol_options.as_str()
        } else {
            "{}"
        },
        if is_encoded {
            muxer_options.as_str()
        } else {
            "{}"
        },
        if is_encoded {
            metadata_options.as_str()
        } else {
            "{}"
        },
        is_encoded.then_some(data.output.audio_codec.as_str()),
        if is_encoded {
            audio_options.as_str()
        } else {
            "{}"
        },
        (is_encoded && noar_engine::audio_codec_uses_bitrate(&data.output.audio_codec))
            .then_some(i64::from(data.output.audio_bitrate)),
        is_encoded.then_some(i64::from(data.output.audio_sample_rate)),
    )
    .await?;
    handles::update_recording_on(&mut transaction, id, &data.recording).await?;
    handles::update_scte35_on(&mut transaction, id, &data.scte35).await?;
    handles::update_configuration_with_projection_on(
        &mut transaction,
        config_id,
        data,
        (config.output.width, config.output.height),
    )
    .await?;
    transaction.commit().await?;
    let new_config = get_config(pool, id).await?;
    let mut queues = mail_queues.lock().await;

    for queue in queues.iter_mut() {
        let mut queue_lock = queue.lock().await;

        if queue_lock.id == id {
            if queue_lock.config.recipient != new_config.mail.recipient {
                queue_lock.clear_raw();
            }

            queue_lock.update(new_config.mail.clone());
            queue_lock.update_notification(new_config.notification.clone());
            break;
        }
    }

    let running_config = manager.running_config();
    let requires_restart =
        requires_playout_restart(running_config.as_deref().unwrap_or(&config), &new_config);
    manager
        .audio_effects
        .set_volume(new_config.audio.volume)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    manager.live_loudness.update(
        running_config
            .as_deref()
            .unwrap_or(&config)
            .audio
            .loudness_scope
            != "off",
        crate::player::controller::loudness_config(&new_config.audio),
    );
    manager.logo_control.set_transitions(logo_transitions);
    crate::player::output::load_lshape(manager, &new_config).await;
    // Sized for the output on air; a new output size restarts anyway.
    let (logo_width, logo_height) = running_config.as_deref().map_or(
        (new_config.output.width, new_config.output.height),
        |running| (running.output.width, running.output.height),
    );
    crate::player::output::load_logo(manager, &new_config, logo_width, logo_height);
    manager.update_config(new_config).await;

    Ok(requires_restart)
}

/// Only volume and mail are consumed by long-lived runtime controls, and the
/// guide logo and logo key are read when used. The engine creates input,
/// compositor and output contexts when playout starts, therefore every other
/// configuration change requires a fresh instance.
fn requires_playout_restart(current: &PlayoutConfig, updated: &PlayoutConfig) -> bool {
    let Ok(mut current) = serde_json::to_value(current) else {
        return true;
    };
    let Ok(mut updated) = serde_json::to_value(updated) else {
        return true;
    };

    for config in [&mut current, &mut updated] {
        let Some(config) = config.as_object_mut() else {
            return true;
        };

        config.remove("mail");
        config.remove("notification");
        if let Some(processing) = config
            .get_mut("processing")
            .and_then(serde_json::Value::as_object_mut)
        {
            processing.remove("guide_logo");
            processing.remove("logo_key");
            processing.remove("live_key");
            // The channel logo is put on air right away when saved.
            for key in [
                "add_logo",
                "logo",
                "logo_scale",
                "logo_opacity",
                "logo_position",
            ] {
                processing.remove(key);
            }
            // The logo control takes these on air right away.
            processing.remove("logo_transition_in");
            processing.remove("logo_transition_out");
            processing.remove("logo_transition_seconds");
            // The L-shape graphic is loaded on air right away.
            processing.remove("lshape");
            processing.remove("lshape_scale");
            processing.remove("lshape_corner");
            // Scenes only apply when called.
            processing.remove("graphics_scenes");
        }
        if let Some(audio) = config
            .get_mut("audio")
            .and_then(serde_json::Value::as_object_mut)
        {
            let scope = audio.get("loudness_scope").cloned();
            audio.clear();
            audio.insert("loudness_scope".to_string(), scope.unwrap_or_default());
        }
    }

    current != updated
}

fn validate_loudness(processing: &crate::utils::config::Audio) -> Result<(), ServiceError> {
    if !matches!(processing.loudness_scope.as_str(), "all" | "live" | "off")
        || !processing.compressor_ratio.is_finite()
        || !(1.0..=10.0).contains(&processing.compressor_ratio)
        || !processing.compressor_threshold_dbfs.is_finite()
        || !(-60.0..=0.0).contains(&processing.compressor_threshold_dbfs)
        || !processing.pause_threshold_dbfs.is_finite()
        || !(-90.0..=-20.0).contains(&processing.pause_threshold_dbfs)
    {
        return Err(ServiceError::BadRequest(
            "Invalid audio dynamics settings".to_string(),
        ));
    }

    let dynamics_ranges = [
        (processing.compressor_attack_ms, 0.1..=50.0),
        (processing.compressor_hold_ms, 0.0..=2000.0),
        (processing.compressor_release_ms, 10.0..=10000.0),
        (processing.compressor_strong_release_ms, 10.0..=10000.0),
        (processing.compressor_knee_db, 0.0..=24.0),
        (processing.pause_hold_ms, 0.0..=5000.0),
        (processing.pause_return_delay_ms, 0.0..=30000.0),
        (processing.loudness_output_max_correction_db, 0.0..=12.0),
        (processing.loudness_output_gain_up_db_per_second, 0.0..=5.0),
        (
            processing.loudness_output_gain_down_db_per_second,
            0.0..=5.0,
        ),
    ];
    if dynamics_ranges
        .iter()
        .any(|(value, range)| !value.is_finite() || !range.contains(value))
        || processing.pause_return_delay_ms < processing.pause_hold_ms
    {
        return Err(ServiceError::BadRequest(
            "Invalid audio timing or output correction settings".to_string(),
        ));
    }

    let values = [
        processing.loudness_target_lufs,
        processing.loudness_dead_band_lu,
        processing.loudness_max_gain_db,
        processing.loudness_max_attenuation_db,
        processing.loudness_gain_up_db_per_second,
        processing.loudness_gain_down_db_per_second,
        processing.loudness_silence_gate_lufs,
        processing.loudness_true_peak_ceiling_dbtp,
    ];
    if values.iter().any(|value| !value.is_finite())
        || processing.loudness_dead_band_lu < 0.0
        || processing.loudness_max_gain_db < 0.0
        || processing.loudness_max_attenuation_db > 0.0
        || processing.loudness_gain_up_db_per_second < 0.0
        || processing.loudness_gain_down_db_per_second < 0.0
        || processing.loudness_true_peak_ceiling_dbtp > 0.0
    {
        return Err(ServiceError::BadRequest(
            "invalid live loudness settings".to_string(),
        ));
    }
    Ok(())
}

/// **Get Output**
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/playout/output/1 -H 'Authorization: Bearer <TOKEN>'
/// ```
///
/// Response is a JSON object
/// A test alert with the values of the alerts page, saved or not.
#[cfg(test)]
mod tests {
    use super::requires_playout_restart;
    use crate::utils::config::PlayoutConfig;

    #[test]
    fn changing_the_external_output_requires_playout_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.output.stream_url = "srt://mediamtx:8890?streamid=publish:output".to_string();

        assert!(requires_playout_restart(&current, &updated));
    }
    #[test]
    fn later_volume_save_still_requires_restart_for_unapplied_output_change() {
        let running = PlayoutConfig::default();
        let mut saved = running.clone();
        saved.output.stream_url = "srt://mediamtx:8890?streamid=publish:output".to_string();
        let mut second_save = saved.clone();
        second_save.audio.volume = 0.75;

        assert!(!requires_playout_restart(&saved, &second_save));
        assert!(requires_playout_restart(&running, &second_save));
    }

    #[test]
    fn notification_and_volume_changes_do_not_require_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.mail.recipient = "ops@example.org".to_string();
        updated.notification.topic = "noar-alerts".to_string();
        updated.audio.volume = 0.75;

        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn guide_logo_and_logo_key_do_not_require_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.processing.guide_logo = "00-assets/logo-guia.png".to_string();
        updated.processing.logo_key = "c0ffee".to_string();

        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn channel_logo_changes_apply_without_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.processing.add_logo = !current.processing.add_logo;
        updated.processing.logo = "00-assets/outra.png".to_string();
        updated.processing.logo_scale = "15%:-1".to_string();
        updated.processing.logo_opacity = 0.5;
        updated.processing.logo_position = "12:12".to_string();

        assert!(!requires_playout_restart(&current, &updated));

        // Output changes still restart the playout.
        updated.output.width += 2;
        assert!(requires_playout_restart(&current, &updated));
    }

    #[test]
    fn logo_transitions_apply_without_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.processing.logo_transition_in = crate::utils::config::LogoTransition::Cut;
        updated.processing.logo_transition_out = crate::utils::config::LogoTransition::Cut;
        updated.processing.logo_transition_seconds = Some(2.5);

        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn logo_transition_length_must_be_a_sensible_number() {
        let mut config = PlayoutConfig::default();

        for seconds in [None, Some(0.2), Some(10.0)] {
            config.processing.logo_transition_seconds = seconds;
            assert!(config.processing.logo_transitions().is_ok(), "{seconds:?}");
        }

        for seconds in [Some(0.0), Some(-1.0), Some(10.5), Some(f64::NAN)] {
            config.processing.logo_transition_seconds = seconds;
            assert!(config.processing.logo_transitions().is_err(), "{seconds:?}");
        }

        config.processing.logo_transition_seconds = Some(1.5);
        config.processing.logo_transition_out = crate::utils::config::LogoTransition::Cut;
        let transitions = config.processing.logo_transitions().unwrap();
        assert_eq!(transitions.show, noar_engine::LogoTransition::Fade);
        assert_eq!(transitions.hide, noar_engine::LogoTransition::Cut);
        assert_eq!(transitions.seconds, Some(1.5));
    }

    #[test]
    fn the_lshape_is_off_without_a_graphic_and_checks_its_settings() {
        let mut config = PlayoutConfig::default();
        let storage = std::path::Path::new("/storage");

        assert_eq!(config.processing.lshape_config(storage), Ok(None));

        config.processing.lshape = "00-assets/lshape.png".to_string();
        config.processing.lshape_scale = 75.0;
        config.processing.lshape_corner = "top-right".to_string();
        let lshape = config.processing.lshape_config(storage).unwrap().unwrap();
        assert_eq!(lshape.path, "/storage/00-assets/lshape.png");
        assert_eq!(lshape.corner, noar_engine::LShapeCorner::TopRight);

        config.processing.lshape_scale = 99.0;
        assert!(config.processing.lshape_config(storage).is_err());
        config.processing.lshape_scale = 60.0;
        config.processing.lshape_corner = "middle".to_string();
        assert!(config.processing.lshape_config(storage).is_err());

        // Switched on air at once, like the logo transitions.
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.processing.lshape = "00-assets/lshape.png".to_string();
        updated.processing.lshape_scale = 60.0;
        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn loudness_parameter_changes_do_not_require_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.audio.loudness_enable = true;
        updated.audio.loudness_target_lufs = -23.0;

        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn scope_changes_require_restart_but_dynamics_parameters_do_not() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.audio.loudness_scope = "live".to_string();
        assert!(requires_playout_restart(&current, &updated));
        updated.audio.loudness_scope = current.audio.loudness_scope.clone();
        updated.audio.compressor_ratio = 4.0;
        updated.audio.compressor_attack_ms = 10.0;
        updated.audio.compressor_hold_ms = 200.0;
        updated.audio.compressor_release_ms = 2400.0;
        updated.audio.compressor_strong_release_ms = 1000.0;
        updated.audio.compressor_knee_db = 12.0;
        updated.audio.pause_hold_ms = 600.0;
        updated.audio.pause_return_delay_ms = 4000.0;
        updated.audio.loudness_output_max_correction_db = 6.0;
        updated.audio.loudness_output_gain_up_db_per_second = 0.2;
        updated.audio.loudness_output_gain_down_db_per_second = 0.5;

        updated.audio.compressor_threshold_dbfs = -30.0;
        updated.audio.pause_threshold_dbfs = -65.0;
        assert!(!requires_playout_restart(&current, &updated));
    }

    #[test]
    fn dynamics_validation_rejects_invalid_scope_and_detector_parameters() {
        let mut audio = crate::utils::config::Audio {
            loudness_scope: "all".to_string(),
            compressor_ratio: 3.0,
            compressor_attack_ms: 5.0,
            compressor_hold_ms: 100.0,
            compressor_release_ms: 1200.0,
            compressor_strong_release_ms: 500.0,
            compressor_knee_db: 6.0,
            pause_hold_ms: 300.0,
            pause_return_delay_ms: 2000.0,
            loudness_output_max_correction_db: 3.0,
            loudness_output_gain_up_db_per_second: 0.1,
            loudness_output_gain_down_db_per_second: 0.25,

            compressor_threshold_dbfs: -26.0,
            pause_threshold_dbfs: -55.0,
            ..Default::default()
        };
        assert!(super::validate_loudness(&audio).is_ok());
        audio.compressor_attack_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_attack_ms = 51.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_attack_ms = 5.0;
        audio.compressor_hold_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_hold_ms = 2001.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_hold_ms = 100.0;
        audio.compressor_release_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_release_ms = 10001.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_release_ms = 1200.0;
        audio.compressor_strong_release_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_strong_release_ms = 10001.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_strong_release_ms = 500.0;
        audio.compressor_knee_db = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_knee_db = 25.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_knee_db = 6.0;
        audio.pause_hold_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.pause_hold_ms = 5001.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.pause_hold_ms = 300.0;
        audio.pause_return_delay_ms = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.pause_return_delay_ms = 30001.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.pause_return_delay_ms = 2000.0;
        audio.loudness_output_max_correction_db = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_max_correction_db = 13.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_max_correction_db = 3.0;
        audio.loudness_output_gain_up_db_per_second = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_gain_up_db_per_second = 6.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_gain_up_db_per_second = 0.1;
        audio.loudness_output_gain_down_db_per_second = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_gain_down_db_per_second = 6.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_output_gain_down_db_per_second = 0.25;
        audio.pause_return_delay_ms = audio.pause_hold_ms - 1.0;
        assert!(super::validate_loudness(&audio).is_err());
        audio.pause_return_delay_ms = 2000.0;
        audio.loudness_scope = "unknown".to_string();
        assert!(super::validate_loudness(&audio).is_err());
        audio.loudness_scope = "live".to_string();
        audio.compressor_ratio = f64::NAN;
        assert!(super::validate_loudness(&audio).is_err());
        audio.compressor_ratio = 3.0;
        audio.pause_threshold_dbfs = -10.0;
        assert!(super::validate_loudness(&audio).is_err());
    }

    #[test]
    fn output_change_requires_restart() {
        let current = PlayoutConfig::default();
        let mut updated = current.clone();
        updated.output.width = 1920;

        assert!(requires_playout_restart(&current, &updated));

        let mut protocol_update = current.clone();
        protocol_update
            .output
            .protocol_options
            .insert("latency".to_string(), "2000000".to_string());
        assert!(requires_playout_restart(&current, &protocol_update));

        let mut metadata_update = current.clone();
        metadata_update
            .output
            .metadata_options
            .insert("title".to_string(), "Example Program".to_string());
        assert!(requires_playout_restart(&current, &metadata_update));
    }
}
