use std::{env, io, path::PathBuf};

use axum::{
    Json,
    body::Bytes,
    extract::{Multipart, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::Response,
};
use protect_axum::authorities::AuthDetails;
use real::RealIp;
use serde::Deserialize;
use tokio::fs;

use crate::{
    api::{
        auth::decode_jwt,
        file_access::{FileAccessData, FileAccessResponse, check_file_access, prune_file_access},
        routes::{AuthUser, FileObj, ImportObj, ensure_any_authority, stream_file},
        state::AppState,
    },
    db::models::Role,
    file::{MoveObject, PathObject, UploadStatus, UploadStatusQuery, norm_abs_path},
    player::utils::import::import_file,
    utils::{
        commercial::set_commercial,
        errors::ServiceError,
        logo_marks::{LogoMarks, read_marks, write_marks},
        xmltv::sheet::{
            ArtworkKind, MediaSheet, SheetUpdate, read_sheet, remove_artwork, save_artwork,
            save_photo, save_sheet,
        },
    },
};

#[derive(Debug, Deserialize)]
pub struct FileQuery {
    #[serde(default)]
    access: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileAccessRequest {
    filename: String,
}

/// ### File Operations
///
/// **Get File/Folder List**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/browse -H 'Content-Type: application/json'
/// -d '{ "source": "/" }' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn file_browser(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(data): Json<PathObject>,
) -> Result<Json<PathObject>, ServiceError> {
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

    match manager.storage.browser(&data).await {
        Ok(obj) => Ok(Json(obj)),
        Err(e) => Err(e),
    }
}

/// **Create Folder**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/create-folder -H 'Content-Type: application/json'
/// -d '{"source": "<FOLDER PATH>"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn add_dir(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(data): Json<PathObject>,
) -> Result<StatusCode, ServiceError> {
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

    manager.storage.mkdir(&data).await?;

    Ok(StatusCode::OK)
}

/// **Rename File/Folder**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/rename -H 'Content-Type: application/json'
/// -d '{"source": "<SOURCE>", "target": "<TARGET>"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn move_rename(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(data): Json<MoveObject>,
) -> Result<Json<MoveObject>, ServiceError> {
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

    match manager.storage.rename(&data).await {
        Ok(obj) => Ok(Json(obj)),
        Err(e) => Err(e),
    }
}

/// **Remove File/Folder**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/remove -H 'Content-Type: application/json'
/// -d '{"source": "<SOURCE>"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(data): Json<PathObject>,
) -> Result<StatusCode, ServiceError> {
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

    let recursive = data.recursive;

    match manager.storage.remove(&data.source, recursive).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(e) => Err(e),
    }
}

/// **Upload File**
///
/// ```BASH
/// curl -X PUT 'http://127.0.0.1:8787/api/file/1/upload?path=folder' \
/// -H 'Authorization: Bearer <TOKEN>' -F 'fileName=file.mp4' -F 'start=0' \
/// -F 'end=1024' -F 'size=1024' -F 'batch_id=<ID>' \
/// -F 'chunk=@chunk.bin;type=application/octet-stream'
/// ```
pub async fn upload_status(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Query(query): Query<UploadStatusQuery>,
) -> Result<Json<UploadStatus>, ServiceError> {
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

    Ok(Json(manager.storage.upload_status(&query, user.id).await?))
}

pub async fn upload_file(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(obj): Query<FileObj>,
    user: AuthUser,
    details: AuthDetails<Role>,
    payload: Multipart,
) -> Result<StatusCode, ServiceError> {
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

    if let Some(file) = manager.storage.upload(payload, &obj.path, user.id).await? {
        let config = manager.config.read().await.clone();
        manager.conversions.enqueue(&config, &file);
    }

    Ok(StatusCode::OK)
}

/// **Uploaded videos being converted to the output format**
///
/// ```BASH
/// curl http://127.0.0.1:8787/api/file/1/conversions -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_conversions(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::utils::convert::Conversion>>, ServiceError> {
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

    Ok(Json(manager.conversions.list()))
}

pub async fn create_file_access_token(
    real_ip: RealIp,
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<FileAccessRequest>,
) -> Result<Json<FileAccessResponse>, ServiceError> {
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

    let config = manager.config.read().await;
    let storage = config.channel.storage.clone();
    let (path, _, _) = norm_abs_path(&storage, &request.filename)?;
    drop(config);

    let metadata = match fs::metadata(&path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ServiceError::NotFound("File not found".to_string()));
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Err(ServiceError::NotFound("File not found".to_string()));
    }

    let access_data = FileAccessData::new(real_ip.ip().to_string(), user.id, id, &path);
    let response = FileAccessResponse::from(&access_data);
    let mut tokens = state.file_access.tokens.lock().await;
    prune_file_access(&mut tokens);
    tokens.insert(access_data);

    Ok(Json(response))
}

/// **Get File**
///
/// Can be used for preview video files
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/file/1/path/to/file.mp4
/// ```
pub async fn get_file(
    real_ip: RealIp,
    State(state): State<AppState>,
    Path((id, filename)): Path<(i32, String)>,
    Query(query): Query<FileQuery>,
    headers: HeaderMap,
) -> Result<Response, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    let config = manager.config.read().await;
    let storage = config.channel.storage.clone();
    let (path, _, _) = norm_abs_path(&storage, &filename)?;
    drop(config);

    if let Some(token) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        let claims = decode_jwt(token).await?;
        let auth = AuthUser {
            id: claims.id,
            channels: claims.channels,
            role: claims.role,
        };
        auth.ensure_channel_or_admin(id)?;
    } else if let Some(access) = query.access.as_deref() {
        let mut tokens = state.file_access.tokens.lock().await;
        check_file_access(&mut tokens, access, &real_ip.ip().to_string(), id, &path)?;
    } else {
        return Err(ServiceError::Unauthorized("Missing token".to_string()));
    }

    let mut response = stream_file(&path, &headers).await?;

    // Shown in the page (video and image previews), so it needs its real type:
    // Safari, on the iPhone and iPad above all, does not play a video sent as
    // a download. A download link still saves it with its name.
    if let Ok(mime) = HeaderValue::from_str(
        mime_guess::from_path(&path)
            .first_or_octet_stream()
            .essence_str(),
    ) {
        response.headers_mut().insert(CONTENT_TYPE, mime);
    }

    response
        .headers_mut()
        .insert(CONTENT_DISPOSITION, HeaderValue::from_static("inline"));

    Ok(response)
}

/// **Import playlist**
///
/// Import text/m3u file and convert it to a playlist
/// lines with leading "#" will be ignore
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/file/1/import -H 'Authorization: Bearer <TOKEN>'
/// -F "file=@list.m3u"
/// ```
pub async fn import_playlist(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(obj): Query<ImportObj>,
    user: AuthUser,
    details: AuthDetails<Role>,
    mut payload: Multipart,
) -> Result<Json<String>, ServiceError> {
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

    let channel_name = manager.channel.lock().await.name.clone();
    let playlists = manager.config.read().await.channel.playlists.clone();
    let file = obj.file.file_name().unwrap_or_default();
    let path = env::temp_dir().join(file);
    let path_clone = path.clone();

    if let Some(field) = payload.next_field().await? {
        let bytes = field.bytes().await?;
        fs::write(&path, &bytes).await?;
    } else {
        return Err(ServiceError::BadRequest("No file provided".into()));
    }

    let response = import_file(&playlists, &obj.date, Some(channel_name), &path_clone).await?;

    fs::remove_file(path).await?;

    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct SheetRequest {
    /// Storage path of a folder or clip.
    path: String,
}

#[derive(Debug, Deserialize)]
pub struct SheetSaveRequest {
    path: String,
    #[serde(flatten)]
    update: SheetUpdate,
}

#[derive(Debug, Deserialize)]
pub struct ArtworkRequest {
    path: String,
    kind: ArtworkKind,
}

#[derive(Debug, Deserialize)]
pub struct PhotoRequest {
    /// Storage path of the folder or clip whose sheet lists the person.
    path: String,
    /// The person, as written in the credits.
    name: String,
}

/// The storage folder of a channel the user may edit, and `path` inside it.
async fn storage_target(
    state: &AppState,
    id: i32,
    user: &AuthUser,
    details: &AuthDetails<Role>,
    path: &str,
) -> Result<(PathBuf, PathBuf), ServiceError> {
    ensure_any_authority(
        details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;
    let storage = manager.config.read().await.channel.storage.clone();
    let (target, _, _) = norm_abs_path(&storage, path)?;

    Ok((storage, target))
}

/// **Read a sheet**
///
/// The `.nfo` sheet of a program folder, or of a clip, with its artwork.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/sheet -H 'Content-Type: application/json'
/// -d '{"path": "Morning News"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_sheet(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<SheetRequest>,
) -> Result<Json<MediaSheet>, ServiceError> {
    let (storage, target) = storage_target(&state, id, &user, &details, &request.path).await?;

    Ok(Json(read_sheet(&storage, &target).await?))
}

/// **Save a sheet**
///
/// Writes the form fields (keeping other elements of an existing file), or
/// `raw`, the `.nfo` text as it is.
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/file/1/sheet -H 'Content-Type: application/json'
/// -d '{"path": "Morning News", "kind": "program", "fields": {"title": "Morning News"}}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn update_sheet(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<SheetSaveRequest>,
) -> Result<Json<MediaSheet>, ServiceError> {
    let (storage, target) = storage_target(&state, id, &user, &details, &request.path).await?;

    Ok(Json(save_sheet(&storage, &target, request.update).await?))
}

#[derive(Debug, Deserialize)]
pub struct LogoMarksRequest {
    /// The video, relative to the storage.
    path: String,
}

#[derive(Debug, Deserialize)]
pub struct LogoMarksSaveRequest {
    path: String,
    #[serde(flatten)]
    marks: LogoMarks,
}

/// **Read where a video keeps the logo off**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/logo-marks -H 'Content-Type: application/json'
/// -d '{"path": "Programa/Programa 01.mp4"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_logo_marks(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<LogoMarksRequest>,
) -> Result<Json<LogoMarks>, ServiceError> {
    let (_, video) = storage_target(&state, id, &user, &details, &request.path).await?;

    if !video.is_file() {
        return Err(ServiceError::NotFound(format!(
            "{} not found",
            request.path
        )));
    }

    Ok(Json(read_marks(&video).await))
}

/// **Save where a video keeps the logo off, and its ad breaks**
///
/// Stretches in seconds of the video. They apply wherever it plays, from
/// the next time it starts; none removes them.
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/file/1/logo-marks -H 'Content-Type: application/json'
/// -d '{"path": "Programa/Programa 01.mp4", "hide": [{"from": 750, "to": 990.5}]}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn update_logo_marks(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<LogoMarksSaveRequest>,
) -> Result<Json<LogoMarks>, ServiceError> {
    let (_, video) = storage_target(&state, id, &user, &details, &request.path).await?;

    if !video.is_file() {
        return Err(ServiceError::NotFound(format!(
            "{} not found",
            request.path
        )));
    }

    let marks = request.marks.validated()?;

    write_marks(&video, &marks).await?;

    Ok(Json(marks))
}

#[derive(Debug, Deserialize)]
pub struct CommercialRequest {
    /// Storage folder, relative to the storage.
    path: String,
    commercial: bool,
}

/// **Mark a commercials folder**
///
/// Everything in a commercials folder and its subfolders plays as a
/// commercial: no logo, and no entry of its own in the guide.
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/file/1/commercial -H 'Content-Type: application/json'
/// -d '{"path": "Breaks", "commercial": true}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn mark_commercial(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<CommercialRequest>,
) -> Result<StatusCode, ServiceError> {
    let (storage, _) = storage_target(&state, id, &user, &details, &request.path).await?;

    set_commercial(&storage, &request.path, request.commercial).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// **Upload artwork**
///
/// Stores the image of the `file` field as the poster, backdrop or (clips
/// only) still of a folder or clip.
///
/// ```BASH
/// curl -X PUT 'http://127.0.0.1:8787/api/file/1/artwork?path=Morning%20News&kind=poster' \
/// -H 'Authorization: Bearer <TOKEN>' -F 'file=@poster.jpg'
/// ```
pub async fn upload_artwork(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(request): Query<ArtworkRequest>,
    user: AuthUser,
    details: AuthDetails<Role>,
    mut payload: Multipart,
) -> Result<Json<MediaSheet>, ServiceError> {
    let (storage, target) = storage_target(&state, id, &user, &details, &request.path).await?;
    let bytes = image_field(&mut payload).await?;

    Ok(Json(
        save_artwork(&storage, &target, request.kind, &bytes).await?,
    ))
}

/// **Upload a person's photo**
///
/// Stores the image of the `file` field as the photo of a person in the
/// credits, in `.actors/` of the program folder (`Ana_Souza.jpg`, as Kodi
/// names it). The guide links it from the person's credit.
///
/// ```BASH
/// curl -X PUT 'http://127.0.0.1:8787/api/file/1/photo?path=Morning%20News&name=Ana%20Souza' \
/// -H 'Authorization: Bearer <TOKEN>' -F 'file=@ana.jpg'
/// ```
pub async fn upload_photo(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(request): Query<PhotoRequest>,
    user: AuthUser,
    details: AuthDetails<Role>,
    mut payload: Multipart,
) -> Result<Json<MediaSheet>, ServiceError> {
    let (storage, target) = storage_target(&state, id, &user, &details, &request.path).await?;
    let bytes = image_field(&mut payload).await?;

    Ok(Json(
        save_photo(&storage, &target, &request.name, &bytes).await?,
    ))
}

/// The bytes of the `file` field of an image upload.
async fn image_field(payload: &mut Multipart) -> Result<Bytes, ServiceError> {
    while let Some(field) = payload
        .next_field()
        .await
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?
    {
        if field.name() == Some("file") {
            return field
                .bytes()
                .await
                .map_err(|error| ServiceError::BadRequest(error.to_string()));
        }
    }

    Err(ServiceError::BadRequest("No image was sent".to_string()))
}

/// **Remove artwork**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/file/1/artwork/remove -H 'Content-Type: application/json'
/// -d '{"path": "Morning News", "kind": "backdrop"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn delete_artwork(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<ArtworkRequest>,
) -> Result<Json<MediaSheet>, ServiceError> {
    let (storage, target) = storage_target(&state, id, &user, &details, &request.path).await?;

    Ok(Json(remove_artwork(&storage, &target, request.kind).await?))
}
