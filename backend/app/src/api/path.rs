use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post, put},
};

use crate::{
    api::{auth, routes::*, state::AppState},
    file::MAX_UPLOAD_REQUEST_SIZE,
    sse,
    utils::xmltv::sheet::MAX_ARTWORK_BYTES,
};

fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/refresh", post(auth::refresh))
        .route("/verify", post(auth::verify))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth_routes())
        .nest(
            "/api",
            Router::new()
                .merge(sse::routes::api_routes())
                .route("/channel/{id}", get(get_channel).patch(patch_channel))
                .route("/channel", post(add_channel))
                .route("/channel/{id}", delete(remove_channel))
                .route("/channels", get(get_all_channels))
                .route("/global", get(get_global).put(update_global))
                .route("/global/test-mail", post(send_global_test_mail))
                .route("/control/{id}/playout", post(control_playout))
                .route("/control/{id}/audio", put(update_audio_effects))
                .route("/control/{id}/media/current", get(media_current))
                .route("/control/{id}/process", post(process_control))
                .route("/preview/{id}", get(get_preview))
                .route("/file/{id}/browse", post(file_browser))
                .route("/file/{id}/create-folder", post(add_dir))
                .route("/file/{id}/rename", post(move_rename))
                .route("/file/{id}/remove", post(remove))
                .route(
                    "/file/{id}/upload",
                    get(upload_status)
                        .put(upload_file)
                        .layer(DefaultBodyLimit::max(MAX_UPLOAD_REQUEST_SIZE)),
                )
                .route("/file/{id}/sheet", post(get_sheet).put(update_sheet))
                .route(
                    "/file/{id}/artwork",
                    put(upload_artwork).layer(DefaultBodyLimit::max(MAX_ARTWORK_BYTES + 64 * 1024)),
                )
                .route("/file/{id}/artwork/remove", post(delete_artwork))
                .route("/file/{id}/import", put(import_playlist))
                .route("/file/{id}/access-token", post(create_file_access_token))
                .route("/log/{id}", get(get_log))
                .route("/playlist/{id}", get(get_playlist))
                .route("/playlist/{id}", post(save_playlist))
                .route("/playlist/{id}/generate/{date}", post(gen_playlist))
                .route("/playlist/{id}/programs", post(playlist_programs))
                .route("/playlist/{id}/{date}", delete(del_playlist))
                .route(
                    "/playout/config/{id}",
                    get(get_playout_config).put(update_playout_config),
                )
                .route("/playout/config/{id}/test-alert", post(send_test_alert))
                .route("/playout/outputs/{id}", get(get_playout_outputs))
                .route("/playout/codecs/{id}", get(get_playout_codecs))
                .route("/program/{id}", get(get_program))
                .route(
                    "/schedule/{id}",
                    get(get_schedule_day).post(add_schedule_event),
                )
                .route("/schedule/{id}/events", get(get_schedule_events))
                .route("/schedule/{id}/grade", get(get_schedule_grade))
                .route("/schedule/{id}/status", get(get_schedule_status))
                .route("/schedule/{id}/stop", post(stop_schedule_event))
                .route("/schedule/{id}/lives", get(get_lives).post(add_live))
                .route("/schedule/{id}/lives/test", post(test_live_signal))
                .route(
                    "/schedule/{id}/{event_id}",
                    put(update_schedule_event).delete(delete_schedule_event),
                )
                .route(
                    "/schedule/{id}/{event_id}/trigger",
                    post(trigger_schedule_event),
                )
                .route("/setup", get(get_setup_status).post(complete_setup))
                .route("/system/{id}", get(get_system_stat))
                .route("/user", get(get_user))
                .route(
                    "/user/{id}",
                    get(get_by_name).put(update_user).delete(remove_user),
                )
                .route("/user", post(add_user))
                .route("/users", get(get_users)),
        )
        .nest("/data", sse::routes::data_routes())
        .route("/file/{id}/{*filename}", get(get_file))
        .route("/public/{id}/{public}/{*file_stem}", get(get_public))
        .route("/xmltv/{id}/guide.xml", get(get_xmltv))
        .route("/xmltv/{id}/images/{*image}", get(get_xmltv_image))
}
