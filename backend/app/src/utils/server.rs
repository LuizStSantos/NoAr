//! The NoAr process itself: when it started, and starting it again from the
//! web interface without access to the host (or Docker).

use std::{
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use log::*;
use tokio_util::sync::CancellationToken;

use crate::utils::logging::Target;

/// Unix time this process started serving.
static STARTED_AT: LazyLock<i64> = LazyLock::new(|| Utc::now().timestamp());

static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Time for the restart request to get its answer before the server stops.
const RESTART_DELAY: Duration = Duration::from_millis(800);

/// Note the start time; call once when the server comes up.
pub fn mark_started() {
    LazyLock::force(&STARTED_AT);
}

pub fn started_at() -> i64 {
    *STARTED_AT
}

/// Stop the server like a shutdown signal would (every channel stops and its
/// state is kept), then start it again. The channels that were running start
/// again, and a scheduled event on air resumes where it was.
pub fn request_restart(shutdown: CancellationToken) {
    if RESTART_REQUESTED.swap(true, Ordering::SeqCst) {
        return;
    }

    info!(target: Target::Console.as_str(), "Restart requested from the web interface");

    tokio::spawn(async move {
        tokio::time::sleep(RESTART_DELAY).await;
        shutdown.cancel();
    });
}

pub fn restart_requested() -> bool {
    RESTART_REQUESTED.load(Ordering::SeqCst)
}

/// Replace this process with a new run of the same program and arguments.
/// It keeps the process id, so under Docker the container keeps running
/// and no restart policy is needed. Returns only if that fails.
#[cfg(unix)]
pub fn exec_again() -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let program = match std::env::current_exe() {
        Ok(program) => program,
        Err(error) => return error,
    };

    std::process::Command::new(program)
        .args(std::env::args_os().skip(1))
        .exec()
}

#[cfg(not(unix))]
pub fn exec_again() -> std::io::Error {
    std::io::Error::other("restarting is only supported on Linux")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_time_is_set_once() {
        mark_started();
        let first = started_at();
        mark_started();

        assert_eq!(first, started_at());
        assert!(first > 1_700_000_000);
    }

    #[tokio::test]
    async fn a_restart_stops_the_server_after_answering() {
        let shutdown = CancellationToken::new();

        request_restart(shutdown.clone());

        assert!(restart_requested());
        assert!(!shutdown.is_cancelled(), "the request is answered first");

        tokio::time::timeout(Duration::from_secs(3), shutdown.cancelled())
            .await
            .expect("the server is asked to stop");
    }
}
