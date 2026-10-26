//! Looks for a newer NoAr image: the newest stable release on GitHub (the
//! `latest` image), compared with the commit this program was built from.

use std::{
    sync::{LazyLock, PoisonError, RwLock},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use log::*;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::{sync::Mutex, time::sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    db::handles,
    utils::{
        errors::ServiceError,
        updater::{self, UpdaterStatus},
    },
};

/// Where the image is built; a fork can set its own with `NOAR_UPDATE_REPOSITORY`.
const DEFAULT_REPOSITORY: &str = "LuizStSantos/NoAr";
const GITHUB_API: &str = "https://api.github.com";
/// The workflow that publishes the image.
const IMAGE_WORKFLOW: &str = "docker.yml";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The first check lets the channels start first.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(15);
/// A check asked for sooner than this after the last one answers from it:
/// GitHub allows 60 requests an hour without a login.
const CHECK_COOLDOWN: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// Changes listed at most; the count still says how many there are.
const MAX_CHANGES: usize = 30;

/// The commit this program was built from: the image build passes it in.
pub fn installed_commit() -> Option<&'static str> {
    option_env!("NOAR_COMMIT")
        .map(str::trim)
        .filter(|commit| !commit.is_empty())
}

fn repository() -> String {
    std::env::var("NOAR_UPDATE_REPOSITORY")
        .ok()
        .map(|repository| repository.trim().to_string())
        .filter(|repository| repository.split('/').count() == 2)
        .unwrap_or_else(|| DEFAULT_REPOSITORY.to_string())
}

/// What the settings page shows.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateStatus {
    pub version: &'static str,
    /// Commit of the installed program; empty for a build without one.
    pub commit: Option<String>,
    /// When the installed commit was made, Unix milliseconds.
    pub commit_time: Option<i64>,
    /// The newest published image, once a check found it.
    pub latest: Option<Release>,
    /// The newest image is not the installed one.
    pub available: bool,
    /// Unix milliseconds of the last check.
    pub checked_at: Option<i64>,
    /// Why the last check failed.
    pub error: Option<String>,
    pub auto_check: bool,
    pub repository: String,
    /// Install a new version at `update_time` on its own.
    pub auto_update: bool,
    /// Server time, `HH:MM`.
    pub update_time: String,
    /// A one-off install, Unix milliseconds.
    pub scheduled_at: Option<i64>,
    pub updater: UpdaterStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Release {
    /// Its version, such as `1.1.0`.
    pub version: Option<String>,
    pub commit: String,
    /// When the image was published, Unix milliseconds.
    pub published_at: Option<i64>,
    /// Commits since the installed one, newest first, up to 30.
    pub changes: Vec<Change>,
    /// How many commits came since the installed one, when known.
    pub change_count: Option<u32>,
    /// GitHub page with the changes.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Change {
    pub commit: String,
    /// First line of the commit message.
    pub title: String,
}

#[derive(Default)]
struct Checked {
    latest: Option<Release>,
    available: bool,
    checked_at: Option<i64>,
    error: Option<String>,
    commit_time: Option<i64>,
    at: Option<Instant>,
}

static CHECKED: LazyLock<RwLock<Checked>> = LazyLock::new(RwLock::default);
/// One check at a time.
static CHECKING: Mutex<()> = Mutex::const_new(());
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let _ = rustls::crypto::ring::default_provider().install_default();

    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("NoAr/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("update check HTTP client configuration must be valid")
});

/// The state of the last check, the settings and the updater.
pub async fn status(pool: &SqlitePool) -> Result<UpdateStatus, ServiceError> {
    let settings = handles::select_update_settings(pool).await?;
    let updater = updater::probe().await;
    let checked = CHECKED.read().unwrap_or_else(PoisonError::into_inner);

    Ok(UpdateStatus {
        version: env!("CARGO_PKG_VERSION"),
        commit: installed_commit().map(str::to_string),
        commit_time: checked.commit_time,
        latest: checked.latest.clone(),
        available: checked.available,
        checked_at: checked.checked_at,
        error: checked.error.clone(),
        auto_check: settings.auto_check,
        repository: repository(),
        auto_update: settings.auto_update,
        update_time: settings.update_time,
        scheduled_at: settings.scheduled_at,
        updater,
    })
}

/// A newer image than the installed one is published, as of the last check.
pub fn is_available() -> bool {
    CHECKED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .available
}

/// How long ago GitHub last answered a check, if it did since the start.
pub fn checked_ago() -> Option<Duration> {
    CHECKED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .at
        .map(|at| at.elapsed())
}

/// Ask GitHub for the newest image, unless the last check was moments ago.
pub async fn check(pool: &SqlitePool) -> Result<UpdateStatus, ServiceError> {
    let _checking = CHECKING.lock().await;
    let recent = CHECKED
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .at
        .is_some_and(|at| at.elapsed() < CHECK_COOLDOWN);

    if !recent {
        let known_time = CHECKED
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .commit_time;
        let result = find_newest(&repository(), installed_commit(), known_time).await;
        let mut checked = CHECKED.write().unwrap_or_else(PoisonError::into_inner);

        checked.at = Some(Instant::now());
        checked.checked_at = Some(Utc::now().timestamp_millis());

        match result {
            Ok(found) => {
                checked.available = found.available;
                checked.latest = found.latest;
                checked.commit_time = found.commit_time.or(checked.commit_time);
                checked.error = None;
            }
            Err(error) => {
                warn!("Update check failed: {error}");
                checked.error = Some(error);
            }
        }
    }

    status(pool).await
}

/// Check after the start and then every six hours, while the setting is on.
pub fn spawn_checker(pool: SqlitePool, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut wait = FIRST_CHECK_AFTER;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = sleep(wait) => {}
            }

            wait = CHECK_EVERY;

            let auto_check = handles::select_update_settings(&pool)
                .await
                .is_ok_and(|settings| settings.auto_check);

            if auto_check && let Err(error) = check(&pool).await {
                warn!("Update check failed: {error}");
            }
        }
    });
}

#[derive(Debug, Default, PartialEq)]
struct Found {
    latest: Option<Release>,
    available: bool,
    commit_time: Option<i64>,
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// The version a release tag names: `v1.1.0` is `1.1.0`.
fn tag_version(tag: &str) -> String {
    tag.strip_prefix('v').unwrap_or(tag).to_string()
}

#[derive(Deserialize)]
struct WorkflowRuns {
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    head_sha: String,
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct Comparison {
    /// `ahead`, `behind`, `identical` or `diverged`: the newest image against
    /// the installed commit.
    status: String,
    ahead_by: u32,
    /// Oldest first.
    commits: Vec<ComparedCommit>,
    html_url: String,
}

#[derive(Deserialize)]
struct ComparedCommit {
    sha: String,
    commit: CommitDetail,
}

#[derive(Deserialize)]
struct CommitDetail {
    message: String,
    committer: Option<CommitSignature>,
}

#[derive(Deserialize)]
struct CommitSignature {
    date: Option<String>,
}

#[derive(Deserialize)]
struct CommitInfo {
    commit: CommitDetail,
}

async fn find_newest(
    repository: &str,
    installed: Option<&str>,
    known_time: Option<i64>,
) -> Result<Found, String> {
    // GitHub's latest release leaves out release candidates and drafts.
    let Some(stable) =
        get_json::<LatestRelease>(&format!("{GITHUB_API}/repos/{repository}/releases/latest"))
            .await?
    else {
        return Ok(Found::default());
    };
    // Its image is out once the image workflow for its tag succeeded.
    let runs: WorkflowRuns = get_json(&format!(
        "{GITHUB_API}/repos/{repository}/actions/workflows/{IMAGE_WORKFLOW}/runs?branch={}&status=success&per_page=1",
        stable.tag_name
    ))
    .await?
    .ok_or_else(|| format!("{repository} has no image workflow"))?;
    let Some(run) = runs.workflow_runs.into_iter().next() else {
        return Ok(Found::default());
    };
    let version = Some(tag_version(&stable.tag_name));
    let release = |commit: String, published_at, comparison: Option<&Comparison>| Release {
        version: version.clone(),
        ..release(repository, commit, published_at, comparison)
    };
    let published_at = run.updated_at.as_deref().and_then(millis);
    let commit_time = match (installed, known_time) {
        (Some(installed), None) => installed_time(repository, installed).await,
        _ => known_time,
    };
    let Some(installed) = installed else {
        // A build without a commit cannot tell what it lacks.
        return Ok(Found {
            latest: Some(release(run.head_sha, published_at, None)),
            available: false,
            commit_time,
        });
    };

    if same_commit(installed, &run.head_sha) {
        return Ok(Found {
            latest: Some(release(run.head_sha, published_at, None)),
            available: false,
            commit_time,
        });
    }

    let comparison: Option<Comparison> = get_json(&format!(
        "{GITHUB_API}/repos/{repository}/compare/{installed}...{}",
        run.head_sha
    ))
    .await?;

    Ok(match comparison {
        Some(comparison) => {
            let available = matches!(comparison.status.as_str(), "ahead" | "diverged");
            let mut latest = release(run.head_sha, published_at, Some(&comparison));

            if !available {
                latest.changes.clear();
                latest.change_count = Some(0);
            }

            Found {
                latest: Some(latest),
                available,
                commit_time,
            }
        }
        // GitHub does not know the installed commit (a local build): the
        // image is different, but what changed is unknown.
        None => Found {
            latest: Some(release(run.head_sha, published_at, None)),
            available: true,
            commit_time,
        },
    })
}

fn release(
    repository: &str,
    commit: String,
    published_at: Option<i64>,
    comparison: Option<&Comparison>,
) -> Release {
    let changes = comparison
        .map(|comparison| {
            comparison
                .commits
                .iter()
                .rev()
                .take(MAX_CHANGES)
                .map(|compared| Change {
                    commit: compared.sha.clone(),
                    title: compared
                        .commit
                        .message
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    let url = comparison.map_or_else(
        || format!("https://github.com/{repository}/commit/{commit}"),
        |comparison| comparison.html_url.clone(),
    );

    Release {
        version: None,
        change_count: comparison.map(|comparison| comparison.ahead_by),
        changes,
        published_at,
        url,
        commit,
    }
}

async fn installed_time(repository: &str, installed: &str) -> Option<i64> {
    let info: CommitInfo = get_json(&format!(
        "{GITHUB_API}/repos/{repository}/commits/{installed}"
    ))
    .await
    .ok()??;

    info.commit.committer?.date.as_deref().and_then(millis)
}

/// `None` when GitHub answers "not found".
async fn get_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<Option<T>, String> {
    let response = CLIENT
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| format!("GitHub did not answer: {error}"))?;

    match response.status() {
        StatusCode::NOT_FOUND | StatusCode::UNPROCESSABLE_ENTITY => Ok(None),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            Err("GitHub limits how often NoAr can ask; it tries again later".to_string())
        }
        status if !status.is_success() => Err(format!("GitHub answered {status}")),
        _ => response
            .json()
            .await
            .map(Some)
            .map_err(|error| format!("unexpected answer from GitHub: {error}")),
    }
}

/// Short and full forms of a commit name the same commit.
fn same_commit(left: &str, right: &str) -> bool {
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };

    shorter.len() >= 7 && longer.starts_with(shorter)
}

fn millis(time: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(time)
        .ok()
        .map(|time| time.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison(status: &str, titles: &[&str]) -> Comparison {
        Comparison {
            status: status.to_string(),
            ahead_by: titles.len() as u32,
            commits: titles
                .iter()
                .enumerate()
                .map(|(index, title)| ComparedCommit {
                    sha: format!("{index:040}"),
                    commit: CommitDetail {
                        message: format!("{title}\n\nWhy it changed."),
                        committer: None,
                    },
                })
                .collect(),
            html_url: "https://github.com/o/r/compare/a...b".to_string(),
        }
    }

    #[test]
    fn changes_are_listed_newest_first_by_their_first_line() {
        let comparison = comparison("ahead", &["Older change", "Newer change"]);
        let release = release("o/r", "b".repeat(40), Some(1), Some(&comparison));

        assert_eq!(
            release
                .changes
                .iter()
                .map(|change| change.title.as_str())
                .collect::<Vec<_>>(),
            ["Newer change", "Older change"]
        );
        assert_eq!(release.change_count, Some(2));
        assert_eq!(release.url, "https://github.com/o/r/compare/a...b");
    }

    #[test]
    fn a_long_list_keeps_its_count() {
        let titles: Vec<String> = (0..40).map(|index| format!("Change {index}")).collect();
        let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
        let comparison = comparison("ahead", &titles);
        let release = release("o/r", "b".repeat(40), None, Some(&comparison));

        assert_eq!(release.changes.len(), MAX_CHANGES);
        assert_eq!(release.changes[0].title, "Change 39");
        assert_eq!(release.change_count, Some(40));
    }

    #[test]
    fn without_a_comparison_the_release_links_its_commit() {
        let release = release("o/r", "abc1234".to_string(), None, None);

        assert!(release.changes.is_empty());
        assert_eq!(release.change_count, None);
        assert_eq!(release.url, "https://github.com/o/r/commit/abc1234");
    }

    #[test]
    fn short_and_full_commits_match() {
        let full = "02856a01f00d02856a01f00d02856a01f00d0285";

        assert!(same_commit("02856a0", full));
        assert!(same_commit(full, full));
        assert!(!same_commit("02856", full));
        assert!(!same_commit("c20f3f92", full));
    }

    #[test]
    fn a_release_tag_names_its_version() {
        assert_eq!(tag_version("v1.1.0"), "1.1.0");
        assert_eq!(tag_version("1.2.0"), "1.2.0");
    }

    #[test]
    fn github_times_become_milliseconds() {
        assert_eq!(millis("2026-10-20T16:27:45Z"), Some(1_792_513_665_000));
        assert_eq!(millis("yesterday"), None);
    }
}
