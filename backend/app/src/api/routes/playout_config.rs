//! HTTP adapters for channel settings, runtime capability discovery and test alerts.
//!
//! Request authorization stays here; config::update owns the validated save and
//! runtime reconciliation transaction independently of the transport layer.

use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{
        handles,
        models::{Output, Role},
    },
    utils::{
        config::PlayoutConfig, errors::ServiceError, mail::send_mail,
        notification::send_test_notification,
    },
};

#[derive(Debug, Serialize)]
pub struct CodecOption {
    pub name: String,
    pub display_name: String,
    pub codec_id: String,
    pub hardware: bool,
    pub uses_bitrate: bool,
    pub settings: Vec<EncoderSetting>,
}

#[derive(Debug, Serialize)]
pub struct EncoderSetting {
    pub key: String,
    pub label: String,
    pub kind: &'static str,
    pub default: String,
    pub choices: Vec<EncoderSettingChoice>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub visible_when: Option<EncoderSettingVisibility>,
}

#[derive(Debug, Serialize)]
pub struct EncoderSettingChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct EncoderSettingVisibility {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct OutputCodecOptions {
    pub video: Vec<CodecOption>,
    pub audio: Vec<CodecOption>,
}

#[derive(Debug, Serialize)]
pub struct PlayoutCodecOptions {
    pub hls: OutputCodecOptions,
    pub rtmp: OutputCodecOptions,
    pub srt: OutputCodecOptions,
    pub udp: OutputCodecOptions,
    pub custom: OutputCodecOptions,
    pub recording: OutputCodecOptions,
    /// Why NDI output cannot be used on this server; `None` when it can.
    pub ndi_unavailable: Option<String>,
}

/// A video card port on this server.
#[derive(Debug, Serialize)]
pub struct VideoDeviceInfo {
    /// `decklink` or `v4l2`.
    pub kind: &'static str,
    /// `input` or `output`.
    pub direction: &'static str,
    pub name: String,
    /// `decklink://N` or `v4l2:///dev/videoN`.
    pub address: String,
}

/// Video card ports found on this server and the DeckLink modes NoAr plays.
#[derive(Debug, Serialize)]
pub struct VideoDevices {
    pub devices: Vec<VideoDeviceInfo>,
    pub modes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct PlayoutConfigUpdate {
    pub requires_restart: bool,
}

fn codec_option(codec: &noar_engine::MediaCodec) -> CodecOption {
    CodecOption {
        name: codec.name.clone(),
        display_name: codec.display_name.clone(),
        codec_id: codec.codec_id.clone(),
        hardware: codec.hardware,
        uses_bitrate: match codec.media_type {
            noar_engine::MediaKind::Video => noar_engine::video_codec_uses_bitrate(&codec.name),
            noar_engine::MediaKind::Audio => noar_engine::audio_codec_uses_bitrate(&codec.name),
            noar_engine::MediaKind::Subtitle => false,
        },
        settings: match codec.media_type {
            noar_engine::MediaKind::Video => noar_engine::video_option_specs(&codec.name),
            noar_engine::MediaKind::Audio => &[],
            noar_engine::MediaKind::Subtitle => &[],
        }
        .iter()
        .map(|setting| EncoderSetting {
            key: setting.key.to_string(),
            label: setting.label.to_string(),
            kind: match setting.kind {
                noar_engine::VideoOptionKind::Select => "select",
                noar_engine::VideoOptionKind::Number => "number",
            },
            default: setting.default.to_string(),
            choices: setting
                .choices
                .iter()
                .map(|choice| EncoderSettingChoice {
                    value: choice.value.to_string(),
                    label: choice.label.to_string(),
                })
                .collect(),
            minimum: setting.minimum,
            maximum: setting.maximum,
            visible_when: setting
                .visible_when
                .map(|condition| EncoderSettingVisibility {
                    key: condition.key.to_string(),
                    value: condition.value.to_string(),
                }),
        })
        .collect(),
    }
}

fn output_codec_options(target: noar_engine::OutputTarget) -> OutputCodecOptions {
    let capabilities = noar_engine::media_capabilities();

    OutputCodecOptions {
        video: capabilities
            .video_codecs_for(target)
            .iter()
            .map(codec_option)
            .collect(),
        audio: capabilities
            .audio_codecs_for(target)
            .iter()
            .filter(|codec| !codec.hardware)
            .map(codec_option)
            .collect(),
    }
}

fn custom_output_codec_options() -> OutputCodecOptions {
    let capabilities = noar_engine::media_capabilities();

    OutputCodecOptions {
        video: capabilities
            .usable_codecs(noar_engine::MediaKind::Video)
            .iter()
            .map(codec_option)
            .collect(),
        audio: capabilities
            .usable_codecs(noar_engine::MediaKind::Audio)
            .iter()
            .filter(|codec| !codec.hardware)
            .map(codec_option)
            .collect(),
    }
}

/// **Get Config**
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/playout/config/1 -H 'Authorization: Bearer <TOKEN>'
/// ```
///
/// Response is a JSON object
pub async fn get_playout_config(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<PlayoutConfig>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    let config = manager.config.read().await.clone();

    Ok(Json(config))
}

/// **Update Config**
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/playout/config/1 -H "Content-Type: application/json" \
/// -d { <CONFIG DATA> } -H 'Authorization: Bearer <TOKEN>'
/// ```
#[allow(clippy::too_many_arguments)]
pub async fn update_playout_config(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(data): Json<PlayoutConfig>,
) -> Result<Json<PlayoutConfigUpdate>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin, &Role::ChannelAdmin])?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;
    let requires_restart = crate::utils::config::update::apply_config_update(
        &state.pool,
        &manager,
        &state.mail_queues,
        id,
        data,
    )
    .await?;

    Ok(Json(PlayoutConfigUpdate { requires_restart }))
}

#[derive(Debug, Deserialize)]
pub struct KeyRequest {
    /// `renew` for a new key, `clear` for none (the control is off).
    action: String,
}

#[derive(Debug, Serialize)]
pub struct KeyResponse {
    pub key: String,
}

/// **Renew or clear a remote-control key**
///
/// `kind` is `logo` (the logo and L-shape links) or `scte35`. The key is
/// made and saved here, at once, and stays until it is renewed or cleared
/// again: saving the other settings never changes it.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/playout/config/1/keys/logo -H "Content-Type: application/json" \
/// -d '{"action": "renew"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn update_remote_key(
    State(state): State<AppState>,
    AxumPath((id, kind)): AxumPath<(i32, String)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<KeyRequest>,
) -> Result<Json<KeyResponse>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin, &Role::ChannelAdmin])?;
    user.ensure_channel_or_admin(id)?;

    let kind = match kind.as_str() {
        "logo" => handles::RemoteKey::Logo,
        "scte35" => handles::RemoteKey::Scte35,
        "live" => handles::RemoteKey::Live,
        _ => return Err(ServiceError::BadRequest(format!("unknown key '{kind}'"))),
    };
    let key = match request.action.as_str() {
        "renew" => uuid::Uuid::new_v4().simple().to_string(),
        "clear" => String::new(),
        action => {
            return Err(ServiceError::BadRequest(format!(
                "unknown action '{action}'"
            )));
        }
    };
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    handles::update_remote_key(&state.pool, id, kind, &key).await?;

    let mut config = manager.config.read().await.clone();
    match kind {
        handles::RemoteKey::Logo => config.processing.logo_key.clone_from(&key),
        handles::RemoteKey::Scte35 => config.scte35.key.clone_from(&key),
        handles::RemoteKey::Live => config.processing.live_key.clone_from(&key),
    }
    manager.update_config(config).await;

    Ok(Json(KeyResponse { key }))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TestAlert {
    Mail { recipient: String, subject: String },
    Push { topic: String, tags: String },
}

/// **Send a test alert**
///
/// Sends one e-mail or push notification with the given recipient or topic,
/// so the alerts page can check them before saving.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/playout/config/1/test-alert -H "Content-Type: application/json" \
/// -d '{"kind": "push", "topic": "noar-tv", "tags": ""}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn send_test_alert(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(alert): Json<TestAlert>,
) -> Result<&'static str, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin, &Role::ChannelAdmin])?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;
    let config = manager.config.read().await.clone();

    match alert {
        TestAlert::Mail { recipient, subject } => {
            if !config.mail.show {
                return Err(ServiceError::Conflict(
                    "No mail server is configured".to_string(),
                ));
            }

            let mut mail = config.mail;
            mail.recipient = recipient;
            mail.subject = subject;

            send_mail(
                &mail,
                "NoAr test e-mail: the alerts of this channel reach you.".to_string(),
                false,
            )
            .await
            .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
        }
        TestAlert::Push { topic, tags } => {
            if !config.notification.show {
                return Err(ServiceError::Conflict(
                    "No notification server is configured".to_string(),
                ));
            }

            let mut notification = config.notification;
            notification.topic = topic;
            notification.tags = tags;

            send_test_notification(notification, id)
                .await
                .map_err(ServiceError::BadRequest)?;
        }
    }

    Ok("Test sent")
}

pub async fn get_playout_outputs(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<Output>>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    if let Ok(outputs) = handles::select_outputs(&state.pool, id).await {
        return Ok(Json(outputs));
    }

    Err(ServiceError::InternalServerError)
}

pub async fn get_playout_codecs(
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<PlayoutCodecOptions>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    Ok(Json(PlayoutCodecOptions {
        hls: output_codec_options(noar_engine::OutputTarget::Hls),
        rtmp: output_codec_options(noar_engine::OutputTarget::Rtmp),
        srt: output_codec_options(noar_engine::OutputTarget::Srt),
        udp: output_codec_options(noar_engine::OutputTarget::Udp),
        custom: custom_output_codec_options(),
        recording: output_codec_options(noar_engine::OutputTarget::Matroska),
        ndi_unavailable: noar_engine::ndi_unavailable_reason().map(str::to_string),
    }))
}

/// **Video cards**
///
/// DeckLink inputs/outputs and V4L2 capture devices this server sees, and the
/// DeckLink modes. An empty list means no card or driver visible to NoAr.
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/playout/devices/1 -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_video_devices(
    AxumPath(id): AxumPath<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<VideoDevices>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let devices = tokio::task::spawn_blocking(noar_engine::discover_video_devices)
        .await
        .map_err(|error| ServiceError::Conflict(error.to_string()))?
        .map_err(|error| ServiceError::Conflict(format!("{error:#}")))?;

    Ok(Json(VideoDevices {
        devices: devices
            .into_iter()
            .map(|device| VideoDeviceInfo {
                kind: device.kind,
                direction: device.direction,
                name: device.name,
                address: device.address,
            })
            .collect(),
        modes: noar_engine::VIDEO_MODES
            .iter()
            .map(|mode| mode.name)
            .collect(),
    }))
}
