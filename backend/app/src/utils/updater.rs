//! Installs a newer image through the updater container: Watchtower pulls the
//! image and starts NoAr again with it. Only the updater talks to Docker; NoAr
//! only asks it to.

use std::{
    sync::{LazyLock, PoisonError, RwLock},
    time::Duration,
};

use chrono::{Local, NaiveDate, NaiveTime, TimeDelta};
use log::*;
use reqwest::{StatusCode, Url};
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::{
    db::handles,
    utils::{errors::ServiceError, update},
};

/// The image the updater replaces; set `NOAR_UPDATE_IMAGE` for another one.
const DEFAULT_IMAGE: &str = "ghcr.io/luizstsantos/noar";
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// How often the scheduled and automatic installs are looked at.
const TICK: Duration = Duration::from_secs(30);
/// An automatic install checks GitHub again when its last answer is older.
const STALE_CHECK: Duration = Duration::from_secs(60 * 60);
/// A scheduled install found this late (NoAr was off) does not run.
const MISSED_SCHEDULE: Duration = Duration::from_secs(10 * 60);

/// What the settings page shows about the updater.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UpdaterStatus {
    /// The address and the token of the updater are set.
    pub configured: bool,
    /// It answered just now.
    pub connected: bool,
    pub address: Option<String>,
    pub error: Option<String>,
}

struct Updater {
    url: String,
    token: String,
    image: String,
}

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let _ = rustls::crypto::ring::default_provider().install_default();

    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("updater HTTP client configuration must be valid")
});

/// Day of the last automatic install, so one time slot installs once.
static AUTO_DAY: LazyLock<RwLock<Option<NaiveDate>>> = LazyLock::new(RwLock::default);

fn updater() -> Option<Updater> {
    let setting = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    Some(Updater {
        url: setting("NOAR_UPDATER_URL")?
            .trim_end_matches('/')
            .to_string(),
        token: setting("NOAR_UPDATER_TOKEN")?,
        image: setting("NOAR_UPDATE_IMAGE").unwrap_or_else(|| DEFAULT_IMAGE.to_string()),
    })
}

/// Whether the updater is set up and answers.
pub async fn probe() -> UpdaterStatus {
    let address = std::env::var("NOAR_UPDATER_URL")
        .ok()
        .filter(|url| !url.trim().is_empty());
    let Some(updater) = updater() else {
        return UpdaterStatus {
            address,
            ..UpdaterStatus::default()
        };
    };
    let answer = CLIENT
        .get(format!("{}/readyz", updater.url))
        .timeout(PROBE_TIMEOUT)
        .send()
        .await;
    let error = match answer {
        Ok(response) if response.status().is_success() => None,
        Ok(response) => Some(format!("the updater answered {}", response.status())),
        Err(error) => Some(format!("the updater does not answer: {error}")),
    };

    UpdaterStatus {
        configured: true,
        connected: error.is_none(),
        address,
        error,
    }
}

/// Ask the updater to install the newest image. It answers at once; NoAr
/// stops moments later, when the updater replaces it.
pub async fn install() -> Result<(), ServiceError> {
    let updater = updater().ok_or_else(|| {
        ServiceError::Conflict(
            "the updater is not set up (NOAR_UPDATER_URL, NOAR_UPDATER_TOKEN)".into(),
        )
    })?;
    let mut url = Url::parse(&format!("{}/v1/update", updater.url)).map_err(|error| {
        ServiceError::Conflict(format!("NOAR_UPDATER_URL is not an address: {error}"))
    })?;

    url.query_pairs_mut()
        .append_pair("image", &updater.image)
        .append_pair("async", "true");

    let response = CLIENT
        .post(url)
        .bearer_auth(&updater.token)
        .send()
        .await
        .map_err(|error| ServiceError::Conflict(format!("the updater does not answer: {error}")))?;

    match response.status() {
        status if status.is_success() => {
            info!("The updater is installing the newest image of {}", updater.image);

            Ok(())
        }
        StatusCode::UNAUTHORIZED => Err(ServiceError::Conflict(
            "the updater refused the token: NOAR_UPDATER_TOKEN must match its WATCHTOWER_HTTP_API_TOKEN"
                .into(),
        )),
        StatusCode::TOO_MANY_REQUESTS => Err(ServiceError::Conflict(
            "the updater is already installing an update".into(),
        )),
        status => Err(ServiceError::Conflict(format!(
            "the updater answered {status}"
        ))),
    }
}

/// The next time the server clock shows `time` (`HH:MM`), Unix milliseconds.
pub fn next_time(time: &str) -> Result<i64, ServiceError> {
    let time = NaiveTime::parse_from_str(time.trim(), "%H:%M")
        .map_err(|_| ServiceError::BadRequest(format!("{time} is not a time like 03:00")))?;
    let now = Local::now();
    let mut date = now.date_naive();

    if now.time() >= time {
        date += TimeDelta::days(1);
    }

    date.and_time(time)
        .and_local_timezone(Local)
        .earliest()
        .map(|moment| moment.timestamp_millis())
        .ok_or_else(|| {
            ServiceError::BadRequest(format!("{time} does not exist on the server clock"))
        })
}

/// Install at the scheduled moment, and every day at the set time when a new
/// version is out and "install on its own" is on.
pub fn spawn_installer(pool: SqlitePool, shutdown: CancellationToken) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = sleep(TICK) => {}
            }

            if let Err(error) = install_when_due(&pool).await {
                error!("Scheduled update failed: {error}");
            }
        }
    });
}

async fn install_when_due(pool: &SqlitePool) -> Result<(), ServiceError> {
    let mut settings = handles::select_update_settings(pool).await?;
    let now = Local::now();

    if let Some(at) = settings
        .scheduled_at
        .filter(|at| *at <= now.timestamp_millis())
    {
        settings.scheduled_at = None;
        handles::update_update_settings(pool, &settings).await?;

        // NoAr was off at that time: installing now, maybe at prime time,
        // is not what was asked.
        if now.timestamp_millis() - at > MISSED_SCHEDULE.as_millis() as i64 {
            warn!("The update scheduled while NoAr was off was dropped; schedule it again");

            return Ok(());
        }

        info!("Installing the update scheduled for now");

        return install().await;
    }

    let due = settings.auto_update && now.format("%H:%M").to_string() == settings.update_time;
    let done_today =
        *AUTO_DAY.read().unwrap_or_else(PoisonError::into_inner) == Some(now.date_naive());

    if !due || done_today {
        return Ok(());
    }

    *AUTO_DAY.write().unwrap_or_else(PoisonError::into_inner) = Some(now.date_naive());

    if update::checked_ago().is_none_or(|ago| ago > STALE_CHECK) {
        update::check(pool).await?;
    }

    if !update::is_available() {
        return Ok(());
    }

    info!(
        "Installing the new version at {}, as set",
        settings.update_time
    );

    install().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_today_or_tomorrow() {
        let next = next_time("03:00").unwrap();
        let ahead = next - Local::now().timestamp_millis();

        assert!(ahead > 0 && ahead <= 24 * 60 * 60 * 1000, "{ahead}");
        assert!(next_time("3h").is_err());
        assert!(next_time("25:00").is_err());
    }
}
