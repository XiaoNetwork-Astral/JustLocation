use crate::cell_providers::{Http, HttpRequest, HttpResponse, QueryError};
use std::time::{Duration, Instant};

pub fn validate_endpoint(endpoint: &str) -> Result<(), QueryError> {
    let uri: ureq::http::Uri = endpoint.parse().map_err(|_| QueryError::InvalidQuery)?;
    if endpoint.len() > 2048
        || !matches!(uri.scheme_str(), Some("https" | "http"))
        || uri.host().is_none()
        || endpoint.contains(['?', '#', '@'])
    {
        return Err(QueryError::InvalidQuery);
    }
    Ok(())
}

pub struct Network {
    deadline: Instant,
}
impl Default for Network {
    fn default() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(30),
        }
    }
}
impl Http for Network {
    fn send(&mut self, request: HttpRequest) -> Result<HttpResponse, QueryError> {
        validate_endpoint(&request.url)?;
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(QueryError::Network)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(remaining.min(Duration::from_secs(6))))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        let response = if let Some(body) = request.body {
            let mut builder = agent
                .post(&request.url)
                .header("Content-Type", "application/json")
                .header("Accept", "application/json");
            if let Some(token) = request.bearer {
                builder = builder.header("Authorization", format!("Bearer {token}"));
            }
            builder.send(body.to_string())
        } else {
            let mut builder = agent.get(&request.url).header("Accept", "application/json");
            for (key, value) in request.query {
                builder = builder.query(&key, &value);
            }
            if let Some(token) = request.bearer {
                builder = builder.header("Authorization", format!("Bearer {token}"));
            }
            builder.call()
        };
        let mut response = response.map_err(|_| QueryError::Network)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_string()
            .map_err(|_| QueryError::InvalidResponse)?;
        Ok(HttpResponse { status, body })
    }
}
