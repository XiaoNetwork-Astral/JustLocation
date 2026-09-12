use crate::cell_providers::{Downloader, Http, HttpRequest, HttpResponse, QueryError};
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

/// 下载地址的校验：**允许带查询串**。
///
/// <p>不要复用 [`validate_endpoint`]：那个函数是给"地址与查询参数分开传"的在线查询用的，
/// 它明确拒绝含 `?` 的地址。数据集下载的 URL 必须自己带 `token` 与 `file`，
/// 于是会被它判成"参数无效"——请求还没发出去就失败了（2026-09-12 踩过，报错信息还会误导成上游的问题）。
fn validate_download(url: &str) -> Result<(), QueryError> {
    let uri: ureq::http::Uri = url.parse().map_err(|_| QueryError::InvalidQuery)?;
    if url.len() > 2048
        || !matches!(uri.scheme_str(), Some("https" | "http"))
        || uri.host().is_none()
        || url.contains(['#', '@'])
    {
        return Err(QueryError::InvalidQuery);
    }
    Ok(())
}

impl Downloader for Network {
    fn download(&mut self, url: &str, accept: &str) -> Result<Box<dyn std::io::Read + Send>, QueryError> {
        validate_download(url)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // 大文件不能按查询那条 6 秒的预算走；给足时间，但仍要有上限，避免永久挂住。
            .timeout_global(Some(Duration::from_secs(600)))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        let response = agent
            .get(url)
            .header("Accept", accept)
            .call()
            .map_err(|_| QueryError::Network)?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 => return Err(QueryError::Unauthorized),
            429 => return Err(QueryError::RateLimited),
            400 => return Err(QueryError::InvalidQuery),
            _ => return Err(QueryError::Unavailable),
        }
        // 这里**不加 `.limit(..)`**：默认上限是给"整包读进内存"用的，而我们要的是流。
        Ok(Box::new(response.into_body().into_reader()))
    }
}
