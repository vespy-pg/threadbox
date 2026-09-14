//! Bounded recovery for idempotent provider reads.

use std::{thread, time::Duration};

use reqwest::{
    blocking::{RequestBuilder, Response},
    header::RETRY_AFTER,
    StatusCode,
};

use crate::error::{AppError, AppResult};

const MAX_ATTEMPTS: usize = 3;

pub fn send_idempotent(request: RequestBuilder) -> AppResult<Response> {
    let template = request.try_clone().ok_or_else(|| {
        AppError::InvalidInput("This provider request cannot be retried safely".into())
    })?;
    for attempt in 0..MAX_ATTEMPTS {
        let next = template
            .try_clone()
            .ok_or_else(|| AppError::InvalidInput("Could not retry the provider request".into()))?;
        match next.send() {
            Ok(response) if retryable_status(response.status()) && attempt + 1 < MAX_ATTEMPTS => {
                thread::sleep(retry_delay(
                    attempt,
                    response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                ));
            }
            Ok(response) => return Ok(response),
            Err(error) if retryable_transport(&error) && attempt + 1 < MAX_ATTEMPTS => {
                thread::sleep(retry_delay(attempt, None));
            }
            Err(error) if retryable_transport(&error) => {
                return Err(AppError::InvalidInput(
                    "The provider is unreachable. Check the internet connection and try again"
                        .into(),
                ));
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("the bounded retry loop always returns")
}

fn retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
}

fn retryable_transport(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout()
}

fn retry_delay(attempt: usize, retry_after: Option<&str>) -> Duration {
    retry_after
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| Duration::from_secs(seconds.clamp(1, 5)))
        .unwrap_or_else(|| Duration::from_millis(250 * 4_u64.pow(attempt as u32)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_transient_provider_statuses() {
        for status in [429, 502, 503, 504] {
            assert!(retryable_status(StatusCode::from_u16(status).unwrap()));
        }
        for status in [400, 401, 403, 404, 410, 422] {
            assert!(!retryable_status(StatusCode::from_u16(status).unwrap()));
        }
    }

    #[test]
    fn retry_after_is_bounded_and_backoff_is_short() {
        assert_eq!(retry_delay(0, Some("20")), Duration::from_secs(5));
        assert_eq!(retry_delay(0, Some("0")), Duration::from_secs(1));
        assert_eq!(retry_delay(0, None), Duration::from_millis(250));
        assert_eq!(retry_delay(1, None), Duration::from_secs(1));
    }
}
