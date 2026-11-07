use reqwest::Url;
use serde::Deserialize;

const CONFIG: &str = include_str!("../../feedback.config.json");
const VERSION_PLACEHOLDER: &str = "__ZIPSA_VERSION__";
const OS_PLACEHOLDER: &str = "__ZIPSA_OS__";
const INVALID_CONFIG: &str = "의견 접수창을 열 수 없어요. 앱의 폼 연결 설정을 확인해 주세요.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    prefilled_url: String,
}

pub fn platform_label() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        other => other,
    }
}

pub fn form_url(version: &str) -> Result<String, String> {
    let config: Config = serde_json::from_str(CONFIG).map_err(|_| INVALID_CONFIG)?;
    url_from_template(&config.prefilled_url, version, platform_label())
}

fn url_from_template(template: &str, version: &str, os: &str) -> Result<String, String> {
    if template.trim().is_empty() {
        return Err("의견 접수창을 준비하고 있어요.".into());
    }
    let mut url = Url::parse(template.trim()).map_err(|_| INVALID_CONFIG)?;
    let segments: Vec<_> = url.path().split('/').collect();
    let valid_path = match segments.as_slice() {
        ["", "forms", "d", "e", id, "viewform"] | ["", "forms", "d", id, "viewform"] => {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        }
        _ => false,
    };
    if url.scheme() != "https"
        || url.host_str() != Some("docs.google.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !valid_path
    {
        return Err(INVALID_CONFIG.into());
    }

    // Only the two explicitly named metadata fields survive. A copied prefill link
    // must never add a name, email, local path, or other field to outgoing URLs.
    let mut version_field = None;
    let mut os_field = None;
    for (key, value) in url.query_pairs() {
        let Some(id) = key.strip_prefix("entry.") else {
            continue;
        };
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) {
            return Err(INVALID_CONFIG.into());
        }
        let field = match value.as_ref() {
            VERSION_PLACEHOLDER => &mut version_field,
            OS_PLACEHOLDER => &mut os_field,
            _ => continue,
        };
        if field.replace(key.into_owned()).is_some() {
            return Err(INVALID_CONFIG.into());
        }
    }
    let (Some(version_field), Some(os_field)) = (version_field, os_field) else {
        return Err(INVALID_CONFIG.into());
    };
    if version_field == os_field {
        return Err(INVALID_CONFIG.into());
    }
    url.set_fragment(None);
    url.set_query(None);
    url.query_pairs_mut()
        .append_pair("usp", "pp_url")
        .append_pair(&version_field, version)
        .append_pair(&os_field, os);
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "https://docs.google.com/forms/d/e/test-form/viewform?usp=pp_url&entry.123=__ZIPSA_VERSION__&entry.456=__ZIPSA_OS__";

    #[test]
    fn fills_only_version_and_os_and_removes_other_prefilled_data() {
        let template = format!(
            "{TEMPLATE}&entry.789=private-home-name&email=private@example.com#private-fragment"
        );
        let result = url_from_template(&template, "0.2.0+test&extra=value", "macOS").unwrap();
        let parsed = Url::parse(&result).unwrap();
        let query: Vec<_> = parsed.query_pairs().collect();
        assert_eq!(query.len(), 3);
        assert_eq!(
            query[1],
            ("entry.123".into(), "0.2.0+test&extra=value".into())
        );
        assert_eq!(query[2], ("entry.456".into(), "macOS".into()));
        assert!(parsed.fragment().is_none());
        assert!(!result.contains("private"));
    }

    #[test]
    fn accepts_the_legacy_response_page_and_windows_metadata() {
        let template = TEMPLATE.replace("/d/e/test-form/", "/d/test-form/");
        let result = url_from_template(&template, "0.1.1", "Windows").unwrap();
        assert!(result.ends_with("entry.123=0.1.1&entry.456=Windows"));
    }

    #[test]
    fn rejects_other_hosts_protocols_and_non_response_pages() {
        for template in [
            TEMPLATE.replace("https:", "http:"),
            TEMPLATE.replace("docs.google.com", "docs.google.com.example.org"),
            TEMPLATE.replace("docs.google.com", "user@docs.google.com"),
            TEMPLATE.replace("docs.google.com", "docs.google.com:8443"),
            TEMPLATE.replace("viewform", "edit"),
            TEMPLATE.replace("viewform", "formResponse"),
            TEMPLATE.replace(
                "https://docs.google.com/forms/d/e/test-form/viewform",
                "javascript:alert(1)",
            ),
        ] {
            assert!(url_from_template(&template, "0.1.1", "Windows").is_err());
        }
    }

    #[test]
    fn rejects_missing_ambiguous_or_invalid_metadata_fields() {
        for template in [
            String::new(),
            TEMPLATE.replace(OS_PLACEHOLDER, "macOS"),
            TEMPLATE.replace("entry.456", "entry.123"),
            TEMPLATE.replace("entry.456", "entry.invalid"),
            format!("{TEMPLATE}&entry.789={VERSION_PLACEHOLDER}"),
        ] {
            assert!(url_from_template(&template, "0.1.1", "macOS").is_err());
        }
    }

    #[test]
    fn bundled_config_is_empty_or_a_valid_feedback_form() {
        let config: Config = serde_json::from_str(CONFIG).unwrap();
        if !config.prefilled_url.is_empty() {
            assert!(form_url("0.1.1").is_ok());
        }
    }
}
