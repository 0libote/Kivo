use crate::commands::{CommandError, UpdateResult};
use tauri::AppHandle;

pub(crate) async fn check_for_updates(app: &AppHandle) -> Result<UpdateResult, CommandError> {
    #[derive(serde::Deserialize)]
    struct GitHubRelease {
        tag_name: String,
        html_url: Option<String>,
    }

    // Rolling beta manifest published by the CI `publish-continuous` job as
    // `continuous.json` on the `continuous` pre-release. `builtAt` and any
    // future fields are ignored: only `version` + `sha` drive detection.
    #[derive(serde::Deserialize)]
    struct ContinuousManifest {
        version: String,
        #[serde(default)]
        sha: Option<String>,
    }

    let current_version = app.package_info().version.to_string();
    let current_sha = current_build_sha();

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|_| update_check_error())?;
    let user_agent = "Kivo desktop updater";

    // 1. Prefer a newer stable release. Stable installs never follow the beta
    // channel; beta installs continue checking rolling builds once caught up.
    // A 404 just means no stable release has been published yet.
    let stable = client
        .get("https://api.github.com/repos/0libote/Kivo/releases/latest")
        .header("accept", "application/vnd.github+json")
        .header("user-agent", user_agent)
        .send()
        .await
        .map_err(|_| update_check_error())?;
    if stable.status().is_success() {
        let release = stable
            .json::<GitHubRelease>()
            .await
            .map_err(|_| update_check_error())?;
        let available_version = stable_version_from_tag(&release.tag_name);
        // A mistagged stable release (tag not starting with `app-v`) must not
        // read as "up to date": surface it as a check failure instead.
        let available_version = available_version.ok_or_else(update_check_error)?;
        let available = version_is_newer(&available_version, &current_version);
        if available || current_sha.is_none() {
            return Ok(UpdateResult {
                current_version,
                available_version: Some(available_version),
                available,
                download_url: Some(
                    release
                        .html_url
                        .unwrap_or_else(|| STABLE_RELEASES_URL.to_owned()),
                ),
                channel: Some("stable".into()),
                current_sha,
                available_sha: None,
            });
        }
    } else if stable.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(update_check_error());
    }

    // 2. For beta installs, or before the first stable release, check the
    // rolling `continuous` pre-release. The version rarely changes, so a
    // same-version build with a different commit SHA is still an update.
    let continuous = client
        .get("https://github.com/0libote/Kivo/releases/download/continuous/continuous.json")
        .header("accept", "application/json")
        .header("user-agent", user_agent)
        .send()
        .await
        .map_err(|_| update_check_error())?;
    if continuous.status().is_success() {
        let manifest = continuous
            .json::<ContinuousManifest>()
            .await
            .map_err(|_| update_check_error())?;
        let available = beta_is_newer(
            current_sha.as_deref(),
            manifest.sha.as_deref(),
            &manifest.version,
            &current_version,
        );
        return Ok(UpdateResult {
            current_version,
            available_version: Some(
                manifest
                    .version
                    .split('+')
                    .next()
                    .unwrap_or(&manifest.version)
                    .to_owned(),
            ),
            available,
            download_url: Some(CONTINUOUS_RELEASE_URL.to_owned()),
            channel: Some("beta".into()),
            current_sha,
            available_sha: manifest.sha,
        });
    }
    if continuous.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(update_check_error());
    }

    // No stable release and no continuous pre-release yet.
    Ok(UpdateResult {
        current_version,
        available_version: None,
        available: false,
        download_url: None,
        channel: None,
        current_sha,
        available_sha: None,
    })
}

const STABLE_RELEASES_URL: &str = "https://github.com/0libote/Kivo/releases/latest";
const CONTINUOUS_RELEASE_URL: &str = "https://github.com/0libote/Kivo/releases/tag/continuous";

/// Commit SHA stamped into CI beta builds via `KIVO_BUILD_SHA`. Local builds
/// have none, which the beta comparison treats as "definitely not the latest
/// beta" so developers still get offered the download.
fn current_build_sha() -> Option<String> {
    option_env!("KIVO_BUILD_SHA")
        .map(str::to_owned)
        .filter(|sha| !sha.is_empty())
}

fn update_check_error() -> CommandError {
    CommandError {
        code: "update_check_failed".into(),
        message: "Kivo couldn’t check for updates right now.".into(),
        recoverable: true,
    }
}

/// Install a signed stable or continuous update. The channel and availability
/// are checked again here so the installer cannot silently switch channels
/// when one manifest is temporarily missing.
pub(super) async fn install(app: AppHandle) -> Result<(), CommandError> {
    use tauri_plugin_updater::UpdaterExt;
    let offered = check_for_updates(&app).await?;
    if !offered.available {
        return Err(update_install_error(
            "update_not_available",
            "No newer update was found. Check again in a moment.",
        ));
    }
    let endpoint = if offered.channel.as_deref() == Some("beta") {
        "https://github.com/0libote/Kivo/releases/download/continuous/continuous.json"
    } else {
        "https://github.com/0libote/Kivo/releases/latest/download/latest.json"
    };
    let updater = app
        .updater_builder()
        .endpoints(vec![
            endpoint.parse().expect("static updater endpoint is valid"),
        ])
        .map_err(|error| {
            update_install_error(
                "update_install_unavailable",
                &format!("The update source is unavailable ({error})."),
            )
        })?;
    let updater = if offered.channel.as_deref() == Some("beta") {
        let current_sha = offered.current_sha;
        updater.version_comparator(move |current, release| {
            // Build metadata identifies rolling builds while preserving the
            // app's public version. Semver's default comparison ignores it.
            let remote_sha = release.version.build.as_str();
            (release.version > current
                || (release.version.major == current.major
                    && release.version.minor == current.minor
                    && release.version.patch == current.patch
                    && release.version.pre == current.pre))
                && !remote_sha.is_empty()
                && current_sha.as_deref() != Some(remote_sha)
        })
    } else {
        updater
    };
    let updater = updater.build().map_err(|error| {
        update_install_error(
            "update_install_unavailable",
            &format!("This build can’t install updates itself ({error})."),
        )
    })?;
    let update = updater
        .check()
        .await
        .map_err(|error| {
            update_install_error(
                "update_install_unavailable",
                &format!(
                    "The update service couldn’t prepare this update ({error}). Use the download link instead.",
                ),
            )
        })?
        .ok_or_else(|| {
            // `check_for_updates` (GitHub API) and the updater plugin
            // (`latest.json`) can disagree briefly after a release is tagged
            // but before its artifacts finish uploading. Point at the manual
            // download instead of claiming "up to date" so the UI, which just
            // offered an install, does not contradict itself.
            update_install_error(
                "update_not_available",
                "No installable update was found. Use the download link instead.",
            )
        })?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| {
            update_install_error(
                "update_install_failed",
                &format!(
                    "The update couldn’t be installed ({error}). Use the download link instead.",
                ),
            )
        })?;
    Ok(())
}

/// Relaunch after an in-app install. The Windows installer exits the app
/// itself; other development hosts finish the update with this restart.
fn update_install_error(code: &str, message: &str) -> CommandError {
    CommandError {
        code: code.into(),
        message: message.into(),
        recoverable: code != "update_not_available",
    }
}

pub(super) fn stable_version_from_tag(tag: &str) -> Option<String> {
    tag.strip_prefix("app-v").map(str::to_owned)
}

pub(super) fn version_is_newer(candidate: &str, current: &str) -> bool {
    fn parts(version: &str) -> Option<Vec<u64>> {
        version
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    }
    matches!((parts(candidate), parts(current)), (Some(candidate), Some(current)) if candidate > current)
}

/// Whether the rolling beta described by the manifest is newer than the
/// running build. A bumped version is always newer; otherwise any commit
/// difference counts (same-version rebuilds are the normal beta case). A
/// build without an embedded SHA (local dev) is treated as outdated whenever
/// a beta manifest exists so the download is still offered.
pub(super) fn beta_is_newer(
    current_sha: Option<&str>,
    manifest_sha: Option<&str>,
    manifest_version: &str,
    current_version: &str,
) -> bool {
    let manifest_version = manifest_version
        .split('+')
        .next()
        .unwrap_or(manifest_version);
    if version_is_newer(manifest_version, current_version) {
        return true;
    }
    if manifest_version != current_version {
        return false;
    }
    match (current_sha, manifest_sha) {
        (Some(current), Some(manifest)) => current != manifest,
        (None, Some(_)) => true,
        _ => false,
    }
}
