use std::env;

const DEFAULT_HOST: &str = "https://healthcheck.mvdm.io";

/// The HealthCheck host: `HEALTHCHECK_URL` when set, without a trailing slash.
pub fn host() -> String {
    match env::var("HEALTHCHECK_URL") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}
