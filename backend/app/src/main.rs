use std::{
    env,
    net::SocketAddr,
    sync::{Arc, LazyLock, atomic::AtomicBool},
};

#[cfg(feature = "processing-bench")]
use std::time::Duration;

use axum::{Router, middleware};
use lazy_limit::{Duration as LDuration, RuleConfig, init_rate_limiter};
use log::*;
use protect_axum::GrantsLayer;
use tokio::{
    fs::File,
    io::AsyncReadExt,
    net::TcpListener,
    sync::{Mutex, RwLock},
};
use tokio_util::sync::CancellationToken;

#[cfg(feature = "dev-metrics")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use noar::{
    ARGS,
    api::file_access::FileAccessState,
    api::{self, state::AppState},
    db::{db_drop, db_pool, handles, init_globales},
    extract,
    middleware::{client_ip::client_ip_layer, governor::rate_limit},
    player::{
        controller::{ChannelController, ChannelManager},
        utils::{JsonPlaylist, get_date, is_remote, json_validate::validate_playlist},
    },
    sse::{SseAuthState, broadcast::Broadcaster},
    utils::{
        args_parse::init_args,
        channels::initialize_channels,
        config::get_config,
        errors::ProcessError,
        logging::{Target, init_logging, log_middleware},
        mail::{self, MailQueue},
        playlist::generate_playlist,
        server,
        system::SystemStat,
        time_machine::set_mock_time,
        update, updater,
    },
};

#[cfg(not(debug_assertions))]
use noar::serve::routes::admin_ui_routes;

fn env_parse_or<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

pub static MAX_BLOCKING_THREADS: LazyLock<usize> =
    LazyLock::new(|| env_parse_or("MAX_BLOCKING_THREADS", 16));

fn main() -> Result<(), ProcessError> {
    #[cfg(feature = "dev-metrics")]
    tracing_subscriber::registry()
        // Do not install console_subscriber's formatting layer. Winit emits
        // high-frequency tracing spans that are not application log records.
        .with(
            console_subscriber::ConsoleLayer::builder()
                .with_default_env()
                .spawn(),
        )
        .init();

    #[cfg(any(feature = "desktop", feature = "desktop-cpu"))]
    noar_engine::run_desktop_on_main_thread(run_async_main)
        .map_err(|error| ProcessError::Custom(format!("Desktop runtime failed: {error:#}")))??;

    #[cfg(not(any(feature = "desktop", feature = "desktop-cpu")))]
    run_async_main()?;

    if server::restart_requested() {
        // Everything is stopped and flushed: run the program again.
        let error = server::exec_again();

        return Err(ProcessError::Custom(format!("Restart failed: {error}")));
    }

    Ok(())
}

/// Panics go to the log with where they happened, then the default report
/// runs; the playout supervisor restarts a channel whose task panicked.
fn log_panics() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown cause".to_string());
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();
        let thread = std::thread::current();

        error!(
            "Panic in thread {}: {message} ({location})",
            thread.name().unwrap_or("unnamed")
        );
        default_hook(info);
    }));
}

fn run_async_main() -> Result<(), ProcessError> {
    log_panics();

    tokio::runtime::Builder::new_multi_thread()
        .max_blocking_threads(*MAX_BLOCKING_THREADS)
        .name("ff-tokio-runtime")
        .thread_name("ff-tokio-worker")
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> Result<(), ProcessError> {
    #[cfg(feature = "processing-bench")]
    noar_engine::set_processing_bench_interval(Duration::from_secs(
        ARGS.processing_bench_report_interval(),
    ));

    let pool = db_pool().await?;

    let init = init_args(&pool).await?;

    if ARGS.init {
        return Ok(());
    }

    set_mock_time(&ARGS.fake_time)?;
    init_globales(&pool).await?;
    let system = SystemStat::new();
    let shutdown = CancellationToken::new();

    let app_state = AppState {
        auth_state: Arc::new(SseAuthState::default()),
        broadcaster: Broadcaster::create(system.clone()),
        controller: Arc::new(RwLock::new(ChannelController::new())),
        file_access: Arc::new(FileAccessState::default()),
        mail_queues: Arc::new(Mutex::new(vec![])),
        pool: pool.clone(),
        shutdown: shutdown.clone(),
        system: system.clone(),
    };

    // Logger handle should be kept alive until the end.
    let _logger = init_logging(app_state.mail_queues.clone())?;

    if let Some(conn) = &ARGS.listen {
        let setup_required = !handles::select_global(&pool).await?.setup_completed;

        if !setup_required {
            initialize_channels(
                &pool,
                app_state.controller.clone(),
                app_state.mail_queues.clone(),
                shutdown.clone(),
                system.clone(),
                init,
            )
            .await?;
        }

        init_rate_limiter!(
            default: RuleConfig::new(LDuration::seconds(1), 16), // 16 req/s globally
            max_memory: Some(64 * 1024 * 1024), // 64MB max memory
            routes: [
                ("/auth/", RuleConfig::new(LDuration::minutes(1), 3).match_prefix(true)), // 3 req/min
                // A refresh needs a valid signed refresh token, so it cannot be
                // guessed; a tight limit would log out several operators who
                // share one address (an exact route wins over the prefix).
                ("/auth/refresh", RuleConfig::new(LDuration::minutes(1), 30)),
            ]
        )
        .await;

        let listener = TcpListener::bind(conn)
            .await
            .map_err(|e| ProcessError::Input(format!("Failed to bind {conn}: {e}")))?;

        let app = Router::new()
            .merge(api::path::routes())
            .with_state(app_state.clone())
            // Layers run bottom-up: the client address is known first, then the
            // request is logged, the user identified, and only then rate limited,
            // so signed-in users are exempt and clients cannot pick their address.
            .layer(middleware::from_fn(rate_limit))
            .layer(GrantsLayer::with_extractor(extract))
            .layer(middleware::from_fn(log_middleware))
            .layer(middleware::from_fn(client_ip_layer));

        #[cfg(not(debug_assertions))]
        let app = app.merge(admin_ui_routes());

        server::mark_started();
        update::spawn_checker(pool.clone(), shutdown.clone());
        updater::spawn_installer(pool.clone(), shutdown.clone());
        info!(target: Target::Console.as_str(), "Running NoAr, listen on http://{conn}");

        tokio::select! {
            result = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()) => {
                result.map_err(|e| ProcessError::Custom(e.to_string()))?;
            }
            _ = shutdown_signal() => {}
            _ = shutdown.cancelled() => {
                info!(target: Target::Console.as_str(), "Shutdown requested");
            }
        }
    } else if ARGS.drop_db {
        db_drop().await;
    } else if let Some(channel_ids) = &ARGS.channel {
        for (index, channel_id) in channel_ids.iter().enumerate() {
            let config = get_config(&pool, *channel_id).await?;
            let channel = handles::select_channel(&pool, channel_id).await?;
            let manager = ChannelManager::new(
                pool.clone(),
                channel,
                config.clone(),
                shutdown.clone(),
                system.clone(),
            )
            .await?;

            if ARGS.foreground {
                let m_queue = Arc::new(Mutex::new(MailQueue::new(
                    *channel_id,
                    config.mail,
                    config.notification,
                )));

                app_state.controller.write().await.add(manager.clone());
                app_state.mail_queues.lock().await.push(m_queue);

                manager.foreground_start(index).await?;
            } else if ARGS.generate.is_some() {
                // Run a simple playlist generator and save it to disk.
                generate_playlist(manager, None, true).await?;
            } else if ARGS.validate {
                let mut playlist_path = config.channel.playlists.clone();
                let start_sec = config.playlist.start_sec.unwrap_or_default();
                let date = get_date(false, start_sec, false, &config.channel.timezone);

                if playlist_path.is_dir() || is_remote(&playlist_path.to_string_lossy()) {
                    let d: Vec<&str> = date.split('-').collect();
                    playlist_path = playlist_path
                        .join(d[0])
                        .join(d[1])
                        .join(date.clone())
                        .with_extension("json");
                }

                debug!("Read: <span class=\"log-addr\">{playlist_path:?}</span>");

                let mut f = File::options()
                    .read(true)
                    .write(false)
                    .open(&playlist_path)
                    .await?;

                let mut contents = String::new();
                f.read_to_string(&mut contents).await?;

                let playlist: JsonPlaylist = serde_json::from_str(&contents)?;

                validate_playlist(
                    config,
                    Arc::new(Mutex::new(Vec::new())),
                    playlist,
                    Arc::new(AtomicBool::new(true)),
                    CancellationToken::new(),
                )
                .await;
            } else if ARGS.test_mail {
                mail::send_mail(
                    &config.mail,
                    "This is just a test email...".to_string(),
                    false,
                )
                .await?;
            }
        }
    } else if !ARGS.user_set {
        error!(
            target: Target::Console.as_str(),
            "Run noar with correct parameters! For example:\n    -l 127.0.0.1:8787\n    --channel 1 2 --foreground\n    --channel 1 --generate 2025-01-20 - 2025-01-25\nRun noar -h for more information."
        );
    }

    let managers = app_state.controller.read().await.managers.clone();

    for manager in &managers {
        manager.channel.lock().await.active = false;
        manager.stop_all(false).await;
        manager.stop_supervisor().await;
    }

    pool.close().await;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            error!(target: Target::Console.as_str(), "Failed to listen for Ctrl-C: {error}");
        }
    };

    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to listen for SIGTERM");

        tokio::select! {
            _ = ctrl_c => info!(target: Target::Console.as_str(), "Shutdown signal received"),
            _ = terminate.recv() => info!(target: Target::Console.as_str(), "Shutdown signal received"),
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
        info!(target: Target::Console.as_str(), "Shutdown signal received");
    }
}
