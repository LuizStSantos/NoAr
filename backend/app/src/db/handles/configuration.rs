use sqlx::{
    Row, SqliteConnection,
    sqlite::{SqlitePool, SqliteQueryResult},
};

use crate::{
    db::models::Configuration,
    utils::{config::PlayoutConfig, errors::ProcessError},
};

pub async fn select_configuration(
    pool: &SqlitePool,
    channel: i32,
) -> Result<Configuration, ProcessError> {
    const QUERY: &str = "SELECT
        c.id, c.channel_id,
        general.stop_threshold AS general_stop_threshold,
        mail.subject AS mail_subject, mail.recipient AS mail_recipient,
        mail.level AS mail_level, mail.interval AS mail_interval,
        notification.topic AS notification_topic,
        notification.level AS notification_level,
        notification.tags AS notification_tags,
        logging.media_level AS logging_media_level,
        logging.ingest_level AS logging_ingest_level,
        logging.detect_silence AS logging_detect_silence,
        logging.ignore_lines AS logging_ignore,
        processing.mode AS processing_mode,
        0 AS processing_add_logo,
        '' AS processing_logo,
        '' AS processing_logo_scale,
        1.0 AS processing_logo_opacity,
        '' AS processing_logo_position,
        audio.volume AS processing_volume,
        audio.loudness_scope AS processing_loudness_scope,
        audio.compressor_ratio AS processing_compressor_ratio,
        audio.compressor_attack_ms AS processing_compressor_attack_ms,
        audio.compressor_hold_ms AS processing_compressor_hold_ms,
        audio.compressor_release_ms AS processing_compressor_release_ms,
        audio.compressor_strong_release_ms AS processing_compressor_strong_release_ms,
        audio.compressor_knee_db AS processing_compressor_knee_db,
        audio.pause_hold_ms AS processing_pause_hold_ms,
        audio.pause_return_delay_ms AS processing_pause_return_delay_ms,
        audio.loudness_output_max_correction_db AS processing_loudness_output_max_correction_db,
        audio.loudness_output_gain_up_db_per_second AS processing_loudness_output_gain_up_db_per_second,
        audio.loudness_output_gain_down_db_per_second AS processing_loudness_output_gain_down_db_per_second,

        audio.compressor_threshold_dbfs AS processing_compressor_threshold_dbfs,
        audio.pause_threshold_dbfs AS processing_pause_threshold_dbfs,
        (audio.loudness_scope != 'off') AS processing_loudness_enable,
        audio.loudness_target_lufs AS processing_loudness_target_lufs,
        audio.loudness_dead_band_lu AS processing_loudness_dead_band_lu,
        audio.loudness_max_gain_db AS processing_loudness_max_gain_db,
        audio.loudness_max_attenuation_db AS processing_loudness_max_attenuation_db,
        audio.loudness_gain_up_db_per_second AS processing_loudness_gain_up_db_per_second,
        audio.loudness_gain_down_db_per_second AS processing_loudness_gain_down_db_per_second,
        audio.loudness_silence_gate_lufs AS processing_loudness_silence_gate_lufs,
        audio.loudness_true_peak_ceiling_dbtp AS processing_loudness_true_peak_ceiling_dbtp,
        processing.vtt_enable AS processing_vtt_enable,
        processing.vtt_dummy AS processing_vtt_dummy,
        processing.vtt_name AS processing_vtt_name,
        processing.vtt_language AS processing_vtt_language,
        processing.vtt_default AS processing_vtt_default,
        processing.guide_logo AS processing_guide_logo,
        processing.logo_key AS processing_logo_key,
        processing.live_key AS processing_live_key,
        'fade' AS processing_logo_transition_in,
        'fade' AS processing_logo_transition_out,
        NULL AS processing_logo_transition_seconds,
        '' AS processing_lshape,
        75.0 AS processing_lshape_scale,
        'top-right' AS processing_lshape_corner,
        '[]' AS processing_graphics_scenes,
        playlist.day_start AS playlist_day_start,
        playlist.length AS playlist_length,
        playlist.infinit AS playlist_infinit,
        storage.filler AS storage_filler,
        storage.extensions AS storage_extensions,
        storage.shuffle AS storage_shuffle,
        storage.convert_uploads AS storage_convert_uploads,
        task.enable AS task_enable, task.path AS task_path,
        output.id AS output_id
    FROM config c
    JOIN config_general general ON general.config_id = c.id
    JOIN config_mail mail ON mail.config_id = c.id
    JOIN config_notification notification ON notification.config_id = c.id
    JOIN config_logging logging ON logging.config_id = c.id
    JOIN config_processing processing ON processing.config_id = c.id
    JOIN config_audio audio ON audio.config_id = c.id
    JOIN config_playlist playlist ON playlist.config_id = c.id
    JOIN config_storage storage ON storage.config_id = c.id
    JOIN config_task task ON task.config_id = c.id
    JOIN config_output output ON output.config_id = c.id AND output.active = 1
    WHERE c.channel_id = $1";

    let mut configuration = sqlx::query_as(QUERY).bind(channel).fetch_one(pool).await?;
    super::project_channel_graphics(pool, &mut configuration).await?;
    Ok(configuration)
}

pub async fn insert_configuration(
    connection: &mut SqliteConnection,
    channel_id: i32,
) -> Result<i32, ProcessError> {
    let config_id: i32 = sqlx::query("INSERT INTO config (channel_id) VALUES ($1) RETURNING id")
        .bind(channel_id)
        .fetch_one(&mut *connection)
        .await?
        .get("id");

    for query in [
        "INSERT INTO config_general (config_id) VALUES ($1)",
        "INSERT INTO config_mail (config_id) VALUES ($1)",
        "INSERT INTO config_notification (config_id) VALUES ($1)",
        "INSERT INTO config_logging (config_id) VALUES ($1)",
        "INSERT INTO config_processing (config_id) VALUES ($1)",
        "INSERT INTO config_audio (config_id) VALUES ($1)",
        "INSERT INTO config_playlist (config_id) VALUES ($1)",
        "INSERT INTO config_storage (config_id) VALUES ($1)",
        "INSERT INTO config_task (config_id) VALUES ($1)",
    ] {
        sqlx::query(query)
            .bind(config_id)
            .execute(&mut *connection)
            .await?;
    }

    Ok(config_id)
}

/// Point a channel's default subtitle file and default logo layers from
/// `00-assets` to `folder`, for a storage set up without the old folder.
/// Paths a user chose stay as they are.
pub async fn point_default_assets_to(
    pool: &sqlx::SqlitePool,
    channel_id: i32,
    folder: &str,
) -> Result<(), ProcessError> {
    let legacy = crate::file::LEGACY_ASSETS_FOLDER;

    if folder == legacy {
        return Ok(());
    }

    let mut transaction = pool.begin().await?;

    sqlx::query(
        "UPDATE config_processing SET vtt_dummy = $2 || '/dummy.vtt'
         WHERE vtt_dummy = $3 || '/dummy.vtt'
           AND config_id IN (SELECT id FROM config WHERE channel_id = $1)",
    )
    .bind(channel_id)
    .bind(folder)
    .bind(legacy)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        r#"UPDATE graphics_templates
         SET layers = replace(layers, '"' || $3 || '/logo.png"', '"' || $2 || '/logo.png"')
         WHERE channel_id = $1"#,
    )
    .bind(channel_id)
    .bind(folder)
    .bind(legacy)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok(())
}

pub async fn update_configuration(
    pool: &SqlitePool,
    id: i32,
    config: PlayoutConfig,
) -> Result<SqliteQueryResult, ProcessError> {
    let mut transaction = pool.begin().await?;
    let result = update_configuration_on(&mut transaction, id, config).await?;
    transaction.commit().await?;
    Ok(result)
}

pub async fn update_configuration_on(
    connection: &mut SqliteConnection,
    id: i32,
    config: PlayoutConfig,
) -> Result<SqliteQueryResult, ProcessError> {
    let dimensions =
        sqlx::query_as("SELECT width,height FROM config_output WHERE config_id=$1 AND active=1")
            .bind(id)
            .fetch_one(&mut *connection)
            .await?;

    update_configuration_with_projection_on(connection, id, config, dimensions).await
}

/// Save a DTO projected before the caller updated its output in the same transaction.
pub async fn update_configuration_with_projection_on(
    connection: &mut SqliteConnection,
    id: i32,
    config: PlayoutConfig,
    projection_dimensions: (u32, u32),
) -> Result<SqliteQueryResult, ProcessError> {
    sqlx::query("UPDATE config_general SET stop_threshold = $2 WHERE config_id = $1")
        .bind(id)
        .bind(config.general.stop_threshold)
        .execute(&mut *connection)
        .await?;
    sqlx::query("UPDATE config_mail SET subject = $2, recipient = $3, level = $4, interval = $5 WHERE config_id = $1")
        .bind(id).bind(config.mail.subject).bind(config.mail.recipient)
        .bind(config.mail.mail_level.as_str()).bind(config.mail.interval)
        .execute(&mut *connection).await?;
    sqlx::query(
        "UPDATE config_notification SET topic = $2, level = $3, tags = $4 WHERE config_id = $1",
    )
    .bind(id)
    .bind(config.notification.topic)
    .bind(config.notification.level.to_string())
    .bind(config.notification.tags)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE config_logging SET media_level = $2, ingest_level = $3, detect_silence = $4, ignore_lines = $5 WHERE config_id = $1")
        .bind(id).bind(config.logging.media_level).bind(config.logging.ingest_level)
        .bind(config.logging.detect_silence).bind(config.logging.ignore_lines.join(";"))
        .execute(&mut *connection).await?;
    super::save_channel_graphics_with_projection_on(
        connection,
        id,
        &config.processing,
        projection_dimensions,
    )
    .await?;
    sqlx::query("UPDATE config_processing SET mode = $2, vtt_enable = $3, vtt_dummy = $4, vtt_name = $5, vtt_language = $6, vtt_default = $7, guide_logo = $8, logo_key = $9 WHERE config_id = $1")
        .bind(id).bind(config.processing.mode.to_string())
        .bind(config.processing.vtt_enable).bind(config.processing.vtt_dummy)
        .bind(config.processing.vtt_name).bind(config.processing.vtt_language)
        .bind(config.processing.vtt_default).bind(config.processing.guide_logo)
        .bind(config.processing.logo_key)
        .execute(&mut *connection).await?;
    sqlx::query("UPDATE config_audio SET volume = $2, loudness_enable = $3, loudness_target_lufs = $4, loudness_dead_band_lu = $5, loudness_max_gain_db = $6, loudness_max_attenuation_db = $7, loudness_gain_up_db_per_second = $8, loudness_gain_down_db_per_second = $9, loudness_silence_gate_lufs = $10, loudness_true_peak_ceiling_dbtp = $11, loudness_scope = $12, compressor_ratio = $13, compressor_threshold_dbfs = $14, pause_threshold_dbfs = $15, compressor_attack_ms = $16, compressor_hold_ms = $17, compressor_release_ms = $18, compressor_strong_release_ms = $19, compressor_knee_db = $20, pause_hold_ms = $21, pause_return_delay_ms = $22, loudness_output_max_correction_db = $23, loudness_output_gain_up_db_per_second = $24, loudness_output_gain_down_db_per_second = $25 WHERE config_id = $1")
        .bind(id).bind(config.audio.volume).bind(config.audio.loudness_scope != "off")
        .bind(config.audio.loudness_target_lufs).bind(config.audio.loudness_dead_band_lu)
        .bind(config.audio.loudness_max_gain_db).bind(config.audio.loudness_max_attenuation_db)
        .bind(config.audio.loudness_gain_up_db_per_second).bind(config.audio.loudness_gain_down_db_per_second)
        .bind(config.audio.loudness_silence_gate_lufs).bind(config.audio.loudness_true_peak_ceiling_dbtp)
        .bind(&config.audio.loudness_scope)
        .bind(config.audio.compressor_ratio)
        .bind(config.audio.compressor_threshold_dbfs)
        .bind(config.audio.pause_threshold_dbfs)
        .bind(config.audio.compressor_attack_ms)
        .bind(config.audio.compressor_hold_ms)
        .bind(config.audio.compressor_release_ms)
        .bind(config.audio.compressor_strong_release_ms)
        .bind(config.audio.compressor_knee_db)
        .bind(config.audio.pause_hold_ms)
        .bind(config.audio.pause_return_delay_ms)
        .bind(config.audio.loudness_output_max_correction_db)
        .bind(config.audio.loudness_output_gain_up_db_per_second)
        .bind(config.audio.loudness_output_gain_down_db_per_second)

        .execute(&mut *connection).await?;
    sqlx::query(
        "UPDATE config_playlist SET day_start = $2, length = $3, infinit = $4 WHERE config_id = $1",
    )
    .bind(id)
    .bind(config.playlist.day_start)
    .bind(config.playlist.length)
    .bind(config.playlist.infinit)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE config_storage SET filler = $2, extensions = $3, shuffle = $4, convert_uploads = $5 WHERE config_id = $1",
    )
    .bind(id)
    .bind(config.storage.filler)
    .bind(config.storage.extensions.join(";"))
    .bind(config.storage.shuffle)
    .bind(config.storage.convert_uploads)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE config_task SET enable = $2, path = $3 WHERE config_id = $1")
        .bind(id)
        .bind(config.task.enable)
        .bind(config.task.path.to_string_lossy().to_string())
        .execute(&mut *connection)
        .await?;

    activate_output(connection, id, config.output.id).await
}

async fn activate_output(
    connection: &mut SqliteConnection,
    config_id: i32,
    output_id: i32,
) -> Result<SqliteQueryResult, ProcessError> {
    // Clear the old selection first. The partial unique index then guarantees
    // that the second statement can never leave two active outputs behind.
    sqlx::query("UPDATE config_output SET active = 0 WHERE config_id = $1")
        .bind(config_id)
        .execute(&mut *connection)
        .await?;
    let result =
        sqlx::query("UPDATE config_output SET active = 1 WHERE config_id = $1 AND id = $2")
            .bind(config_id)
            .bind(output_id)
            .execute(&mut *connection)
            .await?;

    if result.rows_affected() != 1 {
        return Err(ProcessError::Custom(format!(
            "Output {output_id} does not belong to config {config_id}"
        )));
    }

    Ok(result)
}

pub async fn update_configuration_volume(
    pool: &SqlitePool,
    id: i32,
    volume: f64,
) -> Result<SqliteQueryResult, ProcessError> {
    Ok(
        sqlx::query("UPDATE config_audio SET volume = $2 WHERE config_id = $1")
            .bind(id)
            .bind(volume)
            .execute(pool)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::db::handles::db_migrate;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn loudness_migration_preserves_existing_scope_and_defaults_new_rows_to_all() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations")
            .run_to(4, &pool)
            .await
            .unwrap();
        sqlx::query("UPDATE config_audio SET live_loudness_enable = 1 WHERE config_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();
        let scopes: Vec<(i32, String)> =
            sqlx::query_as("SELECT config_id, loudness_scope FROM config_audio ORDER BY config_id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(scopes[0].1, "live");
        assert!(scopes.iter().skip(1).all(|(_, scope)| scope == "off"));
        // Reinsert an existing row using only its key, as channel creation does.
        sqlx::query("DELETE FROM config_audio WHERE config_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO config_audio (config_id) VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        let config = select_configuration(&pool, 1).await.unwrap();
        assert_eq!(config.processing_loudness_scope, "all");
        assert_eq!(config.processing_compressor_ratio, 3.0);
        assert_eq!(config.processing_compressor_attack_ms, 5.0);
        assert_eq!(config.processing_compressor_hold_ms, 100.0);
        assert_eq!(config.processing_compressor_release_ms, 1200.0);
        assert_eq!(config.processing_compressor_strong_release_ms, 500.0);
        assert_eq!(config.processing_compressor_knee_db, 6.0);
        assert_eq!(config.processing_pause_hold_ms, 300.0);
        assert_eq!(config.processing_pause_return_delay_ms, 2000.0);
        assert_eq!(config.processing_loudness_output_max_correction_db, 3.0);
        assert_eq!(config.processing_loudness_output_gain_up_db_per_second, 0.1);
        assert_eq!(
            config.processing_loudness_output_gain_down_db_per_second,
            0.25
        );

        assert_eq!(config.processing_compressor_threshold_dbfs, -26.0);
        assert_eq!(config.processing_pause_threshold_dbfs, -55.0);
    }

    #[tokio::test]
    async fn migration_preserves_non_default_configuration_data() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // Seed the last schema before configuration splitting, then let the
        // regular migrator apply every later migration.
        sqlx::migrate!("../../migrations")
            .run_to(2, &pool)
            .await
            .unwrap();
        sqlx::raw_sql(
            "UPDATE configurations SET general_stop_threshold = 17.5,
                mail_subject = 'Migrated subject', logging_ignore = 'custom warning',
                processing_logo = 'custom/logo.png', ingest_enable = 1,
                ingest_url = 'rtmp://127.0.0.1:1940/live/test',
                playlist_length = '12:00:00', storage_filler = 'custom/filler.mp4',
                text_preset_id = 4, task_enable = 1, task_path = '/opt/task', output_id = 2;
             UPDATE audio_config SET volume = 0.42, live_loudness_enable = 1,
                live_loudness_target_lufs = -19.0, live_loudness_dead_band_lu = 0.75,
                live_loudness_max_gain_db = 5.0, live_loudness_max_attenuation_db = -9.0,
                live_loudness_gain_up_db_per_second = 0.2, live_loudness_gain_down_db_per_second = 1.5,
                live_loudness_silence_gate_lufs = -58.0, live_loudness_true_peak_ceiling_dbtp = -2.0;
             UPDATE outputs SET width = 1920, height = 1080 WHERE id = 2;
             UPDATE recordings SET enabled = 1, source = 'encode', width = 640, height = 360;",
        )
        .execute(&pool)
        .await
        .unwrap();
        db_migrate(&pool).await.unwrap();
        let config = select_configuration(&pool, 1).await.unwrap();
        assert_eq!(config.general_stop_threshold, 17.5);
        assert_eq!(config.mail_subject, "Migrated subject");
        assert_eq!(config.notification_level, "FATAL");
        assert_eq!(config.logging_ignore, "custom warning");
        assert_eq!(config.processing_logo, "custom/logo.png");
        assert_eq!(config.processing_volume, 0.42);
        assert!(config.processing_loudness_enable);
        assert_eq!(config.processing_loudness_scope, "live");
        assert_eq!(config.processing_loudness_target_lufs, -19.0);
        assert_eq!(config.processing_loudness_dead_band_lu, 0.75);
        assert_eq!(config.processing_loudness_max_gain_db, 5.0);
        assert_eq!(config.processing_loudness_max_attenuation_db, -9.0);
        assert_eq!(config.processing_loudness_gain_up_db_per_second, 0.2);
        assert_eq!(config.processing_loudness_gain_down_db_per_second, 1.5);
        assert_eq!(config.processing_loudness_silence_gate_lufs, -58.0);
        assert_eq!(config.processing_loudness_true_peak_ceiling_dbtp, -2.0);
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('config_audio')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(
            !columns
                .iter()
                .any(|name| name.starts_with("live_loudness_"))
        );

        let migrated_listener: (bool, String, i32) = sqlx::query_as(
            "SELECT enabled, identifier, priority FROM config_live_input
             WHERE config_id = $1 AND backend = 'rtmp' AND takeover_mode = 'connection'",
        )
        .bind(config.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(migrated_listener.0);
        assert_eq!(migrated_listener.1, "rtmp://127.0.0.1:1940/live/test");
        assert_eq!(migrated_listener.2, 100);
        assert_eq!(config.playlist_length, "12:00:00");
        assert_eq!(config.storage_filler, "custom/filler.mp4");
        assert!(config.task_enable);
        assert_eq!(config.task_path, "/opt/task");
        assert_eq!(config.output_id, 2);

        let output = super::super::select_outputs(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|output| output.id == 2)
            .unwrap();
        assert_eq!((output.width, output.height), (1920, 1080));
        let recording = super::super::select_recording(&pool, 1).await.unwrap();
        assert!(recording.enabled);
        assert_eq!(recording.source, "encode");
        assert_eq!((recording.width, recording.height), (640, 360));

        let old_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
             ('global', 'roles', 'user', 'user_channels', 'refresh_tokens',
              'outputs', 'configurations', 'audio_config', 'recordings', 'text_presets',
              'config_ingest')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(old_tables, 0);
    }

    #[tokio::test]
    async fn migration_preserves_one_active_output_and_enforces_uniqueness() {
        let pool = pool().await;
        let active: Vec<i32> =
            sqlx::query_scalar("SELECT id FROM config_output WHERE config_id = 1 AND active = 1")
                .fetch_all(&pool)
                .await
                .unwrap();

        assert_eq!(active, [1]);
        assert!(
            sqlx::query("UPDATE config_output SET active = 1 WHERE id = 2")
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn future_pipeline_configuration_has_safe_defaults_and_audio_tracks_are_ordered() {
        let pool = pool().await;
        let output_id: i32 =
            sqlx::query_scalar("SELECT id FROM config_output WHERE config_id = 1 AND active = 1")
                .fetch_one(&pool)
                .await
                .unwrap();

        let output_options: (String, String, String, String, String) = sqlx::query_as(
            "SELECT audio_options, protocol_options, device_backend, device_identifier, device_options
             FROM config_output WHERE id = $1",
        )
        .bind(output_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            output_options,
            (
                "{}".to_string(),
                "{}".to_string(),
                String::new(),
                String::new(),
                "{}".to_string(),
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT program_layout FROM config_audio WHERE config_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "stereo"
        );
        assert_eq!(
            sqlx::query_as::<_, (String, String)>(
                "SELECT demuxer_options, protocol_options FROM config_source WHERE config_id = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            ("{}".to_string(), "{}".to_string())
        );

        let rtmp_input: (i64, String, String, String) = sqlx::query_as(
            "SELECT enabled, backend, identifier, takeover_mode
             FROM config_live_input
             WHERE config_id = 1 AND backend = 'rtmp' AND takeover_mode = 'connection'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            rtmp_input,
            (
                0,
                "rtmp".to_string(),
                "rtmp://127.0.0.1:1936/live/stream".to_string(),
                "connection".to_string(),
            )
        );

        let live_input: (i64, String, f64, f64) = sqlx::query_as(
            "INSERT INTO config_live_input (config_id, priority, backend)
             VALUES (1, 50, 'ndi')
             RETURNING enabled, takeover_mode, signal_loss_grace_seconds, max_duration_seconds",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live_input, (0, "manual".to_string(), 5.0, 0.0));
        let ndi_id: i32 = sqlx::query_scalar(
            "SELECT id FROM config_live_input WHERE config_id = 1 AND backend = 'ndi'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            sqlx::query("INSERT INTO config_live_input (config_id, priority, backend) VALUES (1, 101, 'ndi')")
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query("UPDATE config_live_input SET priority = 101 WHERE id = $1")
                .bind(ndi_id)
                .execute(&pool)
                .await
                .is_err()
        );
        sqlx::query("UPDATE config_live_input SET priority = 100 WHERE id = $1")
            .bind(ndi_id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO config_live_input (config_id, backend, takeover_mode)
             VALUES (1, 'rtmp', 'connection')",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO config_output_audio (output_id, position, language, channel_layout)
             VALUES ($1, 0, 'deu', '5.1')",
        )
        .bind(output_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            sqlx::query("INSERT INTO config_output_audio (output_id, position) VALUES ($1, 0)")
                .bind(output_id)
                .execute(&pool)
                .await
                .is_err()
        );
        sqlx::query(
            "INSERT INTO config_output_audio (output_id, position, default_track)
             VALUES ($1, 1, 1)",
        )
        .bind(output_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            sqlx::query(
                "INSERT INTO config_output_audio (output_id, position, default_track)
                 VALUES ($1, 2, 1)",
            )
            .bind(output_id)
            .execute(&pool)
            .await
            .is_err()
        );
        sqlx::query("INSERT INTO config_output_audio (output_id, position) VALUES ($1, 2)")
            .bind(output_id)
            .execute(&pool)
            .await
            .unwrap();

        let transient_output_id: i32 = sqlx::query_scalar(
            "INSERT INTO config_output (config_id, name) VALUES (1, 'test') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO config_output_audio (output_id, position, default_track)
             VALUES ($1, 0, 1)",
        )
        .bind(transient_output_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM config_output WHERE id = $1")
            .bind(transient_output_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM config_output_audio WHERE output_id = $1",
            )
            .bind(transient_output_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn output_selection_is_switched_atomically() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        activate_output(&mut transaction, 1, 2).await.unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(select_configuration(&pool, 1).await.unwrap().output_id, 2);

        let mut transaction = pool.begin().await.unwrap();
        assert!(
            activate_output(&mut transaction, 1, i32::MAX)
                .await
                .is_err()
        );
        transaction.rollback().await.unwrap();
        assert_eq!(select_configuration(&pool, 1).await.unwrap().output_id, 2);
    }

    #[tokio::test]
    async fn complete_configuration_update_rolls_back_all_topics_on_error() {
        let pool = pool().await;
        sqlx::raw_sql(
            "UPDATE config_global SET logs = 'assets', playlists = 'assets',
                public = 'assets', storage = 'assets';
             UPDATE channels SET public = 'assets', playlists = 'assets', storage = 'assets';",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        let original = select_configuration(&pool, 1).await.unwrap();
        config.general.stop_threshold = 23.5;
        config.mail.subject = "Should roll back".to_string();
        config.notification.topic = "should-roll-back".to_string();
        config.audio.volume = 0.25;
        config.output.id = i32::MAX;

        assert!(update_configuration(&pool, 1, config).await.is_err());
        let unchanged = select_configuration(&pool, 1).await.unwrap();
        assert_eq!(
            unchanged.general_stop_threshold,
            original.general_stop_threshold
        );
        assert_eq!(unchanged.mail_subject, original.mail_subject);
        assert_eq!(unchanged.notification_topic, original.notification_topic);
        assert_eq!(unchanged.processing_volume, original.processing_volume);
        assert_eq!(unchanged.output_id, original.output_id);
    }

    #[tokio::test]
    async fn logo_transitions_are_saved_and_default_to_fades() {
        let pool = pool().await;
        sqlx::raw_sql(
            "UPDATE config_global SET logs = 'assets', playlists = 'assets',
                public = 'assets', storage = 'assets';
             UPDATE channels SET public = 'assets', playlists = 'assets', storage = 'assets';",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();

        assert_eq!(
            config.processing.logo_transition_in,
            crate::utils::config::LogoTransition::Fade
        );
        assert_eq!(config.processing.logo_transition_seconds, None);

        config.processing.logo_transition_out = crate::utils::config::LogoTransition::Cut;
        config.processing.logo_transition_seconds = Some(2.5);
        update_configuration(&pool, 1, config).await.unwrap();

        let restored = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        assert_eq!(
            restored.processing.logo_transition_in,
            crate::utils::config::LogoTransition::Fade
        );
        assert_eq!(
            restored.processing.logo_transition_out,
            crate::utils::config::LogoTransition::Cut
        );
        assert_eq!(restored.processing.logo_transition_seconds, Some(2.5));
    }

    async fn fractional_logo_configuration() -> (SqlitePool, PlayoutConfig, i32) {
        let pool = pool().await;
        let storage =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/storage");
        sqlx::query("UPDATE config_global SET logs='assets',playlists='assets',public='assets',storage='assets'")
            .execute(&pool).await.unwrap();
        sqlx::query("UPDATE channels SET public='assets',playlists='assets',storage=$1")
            .bind(storage.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE config_output SET width=1280,height=720 WHERE active=1")
            .execute(&pool)
            .await
            .unwrap();
        let id: i32 = sqlx::query_scalar(
            "SELECT template_id FROM graphics_channel_bindings WHERE channel_id=1 AND role='logo'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE graphics_templates SET layers=json_set(layers,'$[0].image','logo.png','$[0].x',5.1,'$[0].y',7.3,'$[0].width',9.7,'$[0].height',11.9) WHERE id=$1")
            .bind(id).execute(&pool).await.unwrap();
        let config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        assert_eq!(config.processing.logo_position, "64:52");
        assert_eq!(config.processing.logo_scale, "124:86");

        (pool, config, id)
    }

    async fn saved_logo_geometry(pool: &SqlitePool, id: i32) -> (f64, f64, f64, f64) {
        sqlx::query_as("SELECT json_extract(layers,'$[0].x'),json_extract(layers,'$[0].y'),json_extract(layers,'$[0].width'),json_extract(layers,'$[0].height') FROM graphics_templates WHERE id=$1")
            .bind(id).fetch_one(pool).await.unwrap()
    }

    #[tokio::test]
    async fn unchanged_and_unrelated_configuration_saves_preserve_fractional_logo_geometry() {
        let (pool, config, id) = fractional_logo_configuration().await;
        let geometry = saved_logo_geometry(&pool, id).await;
        update_configuration(&pool, 1, config).await.unwrap();
        assert_eq!(saved_logo_geometry(&pool, id).await, geometry);
        let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        config.audio.volume = 0.25;
        config.processing.logo_opacity = 0.4;
        update_configuration(&pool, 1, config).await.unwrap();
        assert_eq!(saved_logo_geometry(&pool, id).await, geometry);
        let restored = select_configuration(&pool, 1).await.unwrap();
        assert_eq!(restored.processing_volume, 0.25);
        assert_eq!(restored.processing_logo_opacity, 0.4);
    }

    #[tokio::test]
    async fn compatibility_logo_geometry_edits_change_only_the_edited_pair() {
        let (pool, mut config, id) = fractional_logo_configuration().await;
        config.processing.logo_position = "24:36".to_string();
        update_configuration(&pool, 1, config).await.unwrap();
        assert_eq!(
            saved_logo_geometry(&pool, id).await,
            (1.875, 5.0, 9.7, 11.9)
        );
        let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        config.processing.logo_scale = "96:48".to_string();
        update_configuration(&pool, 1, config).await.unwrap();
        assert_eq!(
            saved_logo_geometry(&pool, id).await,
            (1.875, 5.0, 7.5, 100.0 * 48.0 / 720.0)
        );
    }

    #[tokio::test]
    async fn output_resize_compares_against_previous_projection_and_preserves_normalized_logo() {
        let (pool, mut config, id) = fractional_logo_configuration().await;
        let geometry = saved_logo_geometry(&pool, id).await;
        let previous_dimensions = (config.output.width, config.output.height);
        config.output.width = 1920;
        config.output.height = 1080;
        let mut transaction = pool.begin().await.unwrap();
        sqlx::query("UPDATE config_output SET width=1920,height=1080 WHERE id=$1")
            .bind(config.output.id)
            .execute(&mut *transaction)
            .await
            .unwrap();
        update_configuration_with_projection_on(&mut transaction, 1, config, previous_dimensions)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(saved_logo_geometry(&pool, id).await, geometry);
        let restored = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        assert_eq!(restored.processing.logo_position, "98:78");
        assert_eq!(restored.processing.logo_scale, "186:128");
        update_configuration(&pool, 1, restored).await.unwrap();
        db_migrate(&pool).await.unwrap();
        assert_eq!(saved_logo_geometry(&pool, id).await, geometry);
    }

    #[tokio::test]
    async fn saving_configuration_leaves_legacy_listener_rows_inert_and_unchanged() {
        let pool = pool().await;
        sqlx::raw_sql(
            "UPDATE config_global SET logs = 'assets', playlists = 'assets',
                public = 'assets', storage = 'assets';
             UPDATE channels SET public = 'assets', playlists = 'assets', storage = 'assets';",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO config_live_input (config_id, enabled, backend, identifier, takeover_mode)
             VALUES (1, 1, 'srt', 'srt://127.0.0.1:9000', 'connection')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let before: Vec<(i32, bool, String)> = sqlx::query_as(
            "SELECT id, enabled, identifier FROM config_live_input WHERE config_id = 1 ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        assert!(
            serde_json::to_value(&config)
                .unwrap()
                .get("ingest")
                .is_none()
        );
        config.audio.volume = 0.75;
        update_configuration(&pool, 1, config).await.unwrap();
        let restored = PlayoutConfig::new(&pool, 1, None).await.unwrap();
        let after: Vec<(i32, bool, String)> = sqlx::query_as(
            "SELECT id, enabled, identifier FROM config_live_input WHERE config_id = 1 ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(restored.audio.volume, 0.75);
        assert!(
            serde_json::to_value(restored)
                .unwrap()
                .get("ingest")
                .is_none()
        );
        assert_eq!(
            before, after,
            "legacy records are retained only for historical compatibility"
        );
    }
}
