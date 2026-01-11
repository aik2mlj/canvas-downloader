use crate::canvas::ProcessOptions;
use anyhow::{Error, Result};
use rand::Rng;
use reqwest::{header, Response, Url};
use std::time::Duration;

pub async fn get_pages(link: String, options: &ProcessOptions) -> Result<Vec<Response>> {
    fn parse_next_page(resp: &Response) -> Option<String> {
        // Parse LINK header
        let links = resp.headers().get(header::LINK)?.to_str().ok()?; // ok to not have LINK header
        let rels = parse_link_header::parse_with_rel(links).unwrap_or_else(|e| {
            panic!(
                "Error parsing header for next page, uri={}, err={e:?}",
                resp.url()
            )
        });

        // Is last page?
        let nex = rels.get("next")?; // ok to not have "next"
        let cur = rels
            .get("current")
            .unwrap_or_else(|| panic!("Could not find current page for {}", resp.url()));
        let last = rels.get("last")?;
        if cur == last {
            return None;
        };

        // Next page
        Some(nex.raw_uri.clone())
    }

    let mut link = Some(link);
    let mut resps = Vec::new();

    while let Some(uri) = link {
        // GET request
        let resp = get_canvas_api(uri, options).await?;

        // Get next page before returning for json
        link = parse_next_page(&resp);
        resps.push(resp);
    }

    Ok(resps)
}

pub async fn get_canvas_api(url: String, options: &ProcessOptions) -> Result<Response> {
    let mut query_pairs: Vec<(String, String)> = Vec::new();
    // insert into query_pairs from url.query_pairs();
    for (key, value) in Url::parse(&url)?.query_pairs() {
        query_pairs.push((key.to_string(), value.to_string()));
    }
    for retry in 0..3 {
        let resp = options
            .client
            .get(&url)
            .query(&query_pairs)
            .bearer_auth(&options.canvas_token)
            .timeout(Duration::from_secs(10))
            .send()
            .await;

        match resp {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::FORBIDDEN {
                    if retry == 2 {
                        // Log more specific error information on final retry
                        if url.contains("users") {
                            tracing::debug!("Access denied to user data for course - API token may need elevated permissions");
                        } else if url.contains("discussion_topics") {
                            tracing::debug!("Access denied to discussions - course may have restricted discussion access");
                        } else {
                            tracing::debug!(
                                "Access denied to {} - check API token permissions",
                                url
                            );
                        }
                        return Ok(resp);
                    }
                } else {
                    return Ok(resp);
                }
            }
            Err(e) => {
                tracing::error!("Canvas request error uri: {} {}", url, e);
                return Err(e.into());
            }
        }

        // Exponential backoff with jitter: base delay * 2^retry + random jitter
        let base_delay = 500; // 500ms base delay
        let exponential_delay = base_delay * 2_u64.pow(retry);
        let jitter = rand::rng().random_range(0..=exponential_delay / 2);
        let wait_time = Duration::from_millis(exponential_delay + jitter);

        tracing::debug!(
            "Rate limited (403) for {}, waiting {:?} before retry {}/3",
            url,
            wait_time,
            retry + 1
        );
        tokio::time::sleep(wait_time).await;
    }
    Err(Error::msg("canvas request failed"))
}
