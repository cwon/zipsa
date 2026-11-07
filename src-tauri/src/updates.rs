use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/cwon/zipsa/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/cwon/zipsa/releases";
pub const LATEST_RELEASE_PAGE: &str = "https://github.com/cwon/zipsa/releases/latest";

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCheck {
    latest_version: Option<String>,
    update_available: bool,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    body: Option<String>,
}

fn parse_response(current_version: &str, status: u16, body: &str) -> Result<ReleaseCheck, String> {
    match status {
        404 => return Ok(ReleaseCheck::default()),
        403 | 429 => {
            return Err(
                "GitHub 요청이 제한되었어요. 잠시 후 다시 확인하거나 릴리스 페이지를 열어 주세요."
                    .into(),
            )
        }
        200 => {}
        _ => {
            return Err(
                "GitHub에서 버전 정보를 가져오지 못했어요. 잠시 후 다시 확인해 주세요.".into(),
            )
        }
    }
    let release: GitHubRelease = serde_json::from_str(body)
        .map_err(|_| "릴리스 정보를 읽지 못했어요. GitHub 릴리스 페이지에서 확인해 주세요.")?;
    if release.draft || release.prerelease {
        return Ok(ReleaseCheck::default());
    }
    let tag = release.tag_name.trim();
    let tag = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);
    let latest = Version::parse(tag).map_err(|_| {
        "공개된 릴리스의 버전 형식을 읽지 못했어요. 릴리스 페이지에서 확인해 주세요."
    })?;
    let current =
        Version::parse(current_version).map_err(|_| "현재 앱의 버전을 확인하지 못했어요.")?;
    Ok(ReleaseCheck {
        update_available: latest.cmp_precedence(&current).is_gt(),
        latest_version: Some(latest.to_string()),
        notes: release.body.filter(|notes| !notes.trim().is_empty()),
    })
}

fn http_client(current_version: &str) -> Result<reqwest::Client, String> {
    // Reuse ring, keeping the networking stack independent of AWS-LC build tools.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(format!("Zipsa/{current_version}"))
        .timeout(Duration::from_secs(12))
        .https_only(true)
        .build()
        .map_err(|_| "버전 확인을 시작하지 못했어요. 앱을 다시 실행해 주세요.".into())
}

pub async fn check(current_version: &str) -> Result<ReleaseCheck, String> {
    let response = http_client(current_version)?
        .get(LATEST_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .await
        .map_err(|_| "GitHub에 연결하지 못했어요. 인터넷 연결을 확인하고 다시 시도해 주세요.")?;
    let status = response.status().as_u16();
    let body = if status == 200 {
        response
            .text()
            .await
            .map_err(|_| "릴리스 정보를 받지 못했어요. 다시 시도해 주세요.")?
    } else {
        String::new()
    };
    parse_response(current_version, status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_the_https_client_without_a_network_request() {
        assert!(http_client("0.1.0").is_ok());
    }

    fn release(tag: &str) -> String {
        serde_json::json!({"tag_name": tag, "draft": false, "prerelease": false, "body": "새 기능과 개선 사항"}).to_string()
    }

    #[test]
    fn compares_numeric_versions_and_ignores_build_metadata() {
        assert!(
            parse_response("0.9.0", 200, &release("v0.10.0"))
                .unwrap()
                .update_available
        );
        assert!(
            !parse_response("1.0.0", 200, &release("v1.0.0+build.2"))
                .unwrap()
                .update_available
        );
        assert!(
            !parse_response("1.1.0", 200, &release("v1.0.0"))
                .unwrap()
                .update_available
        );
        assert!(
            parse_response("1.0.0-rc.1", 200, &release("1.0.0"))
                .unwrap()
                .update_available
        );
    }

    #[test]
    fn missing_release_and_unpublished_channels_are_not_updates() {
        assert!(parse_response("0.1.0", 404, "Not found")
            .unwrap()
            .latest_version
            .is_none());
        for flag in ["draft", "prerelease"] {
            let mut data: serde_json::Value = serde_json::from_str(&release("v2.0.0")).unwrap();
            data[flag] = true.into();
            assert!(parse_response("0.1.0", 200, &data.to_string())
                .unwrap()
                .latest_version
                .is_none());
        }
    }

    #[test]
    fn reports_rate_limits_and_invalid_release_data_without_offer() {
        for status in [403, 429, 500] {
            assert!(parse_response("0.1.0", status, "").is_err());
        }
        assert!(parse_response("0.1.0", 200, "not JSON").is_err());
        assert!(parse_response("0.1.0", 200, &release("nightly")).is_err());
    }

    #[test]
    fn preserves_release_notes_as_plain_text_and_normalizes_the_tag() {
        let result = parse_response("0.1.0", 200, &release("v0.2.0")).unwrap();
        assert_eq!(result.latest_version.as_deref(), Some("0.2.0"));
        assert_eq!(result.notes.as_deref(), Some("새 기능과 개선 사항"));
    }
}
