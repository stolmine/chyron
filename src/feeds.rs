use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use feed_rs::parser;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::fs;

/// A single headline from an RSS/Atom feed
#[derive(Debug, Clone)]
pub struct Headline {
    pub title: String,
    pub url: Option<String>,
    pub source: String,
    pub published: Option<DateTime<Utc>>,
}

/// Result of validating/fetching a single feed
#[derive(Debug)]
pub struct FeedResult {
    pub status: FeedStatus,
}

#[derive(Debug)]
pub enum FeedStatus {
    Ok { title: String, item_count: usize },
    Error(String),
}

/// Parse a newsboat-style URLs file
/// Format: one URL per line, optional tags after whitespace (ignored)
pub async fn parse_feeds_file(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read feeds file: {}", path.display()))?;

    let urls: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            // Take only the URL part (before any whitespace/tags)
            line.split_whitespace()
                .next()
                .unwrap_or(line)
                .to_string()
        })
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .collect();

    Ok(urls)
}

/// Fetch and parse a single feed, returning headlines
/// Skips headlines that are in the `shown` set to allow deeper feed exhaustion
pub async fn fetch_feed(
    client: &reqwest::Client,
    url: &str,
    max_items: usize,
    max_age: Duration,
    shown: &HashSet<String>,
) -> Result<(String, Vec<Headline>)> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("Failed to fetch feed: {}", url))?;

    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("Failed to read feed body: {}", url))?;

    let feed = parser::parse(&bytes[..])
        .with_context(|| format!("Failed to parse feed: {}", url))?;

    let source = feed
        .title
        .map(|t| t.content)
        .unwrap_or_else(|| url.to_string());

    let now = Utc::now();
    let max_age_chrono = chrono::Duration::from_std(max_age).unwrap_or(chrono::Duration::hours(24));
    let cutoff = now - max_age_chrono;

    let headlines: Vec<Headline> = feed
        .entries
        .into_iter()
        .filter_map(|entry| {
            let title = entry.title.map(|t| t.content)?;
            if title.trim().is_empty() {
                return None;
            }

            let published = entry.published.or(entry.updated);

            // Filter by age if we have a date
            if let Some(pub_date) = published {
                if pub_date < cutoff {
                    return None;
                }
            }

            let url = entry.links.first().map(|l| l.href.clone());

            // Skip already-shown headlines to allow feed exhaustion
            let key = url.as_ref().unwrap_or(&title);
            if shown.contains(key) {
                return None;
            }

            Some(Headline {
                title,
                url,
                source: source.clone(),
                published,
            })
        })
        .take(max_items)
        .collect();

    Ok((source, headlines))
}

/// Validate a feed and return status
pub async fn validate_feed(client: &reqwest::Client, url: &str) -> FeedResult {
    let status = match fetch_feed_status(client, url).await {
        Ok((title, count)) => FeedStatus::Ok {
            title,
            item_count: count,
        },
        Err(e) => FeedStatus::Error(e.to_string()),
    };

    FeedResult { status }
}

async fn fetch_feed_status(client: &reqwest::Client, url: &str) -> Result<(String, usize)> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .with_context(|| "Connection failed")?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {}", response.status());
    }

    let bytes = response.bytes().await.with_context(|| "Failed to read body")?;

    let feed = parser::parse(&bytes[..]).with_context(|| "Invalid feed format")?;

    let title = feed
        .title
        .map(|t| t.content)
        .unwrap_or_else(|| "Untitled".to_string());

    Ok((title, feed.entries.len()))
}

/// Create a configured HTTP client
pub fn create_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("rss-ticker/0.1")
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to create HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_parse_feeds_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "https://example.com/feed.xml").unwrap();
        writeln!(file, "https://example.org/rss \"tag1\" \"tag2\"").unwrap();
        writeln!(file, "# comment").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "https://example.net/atom.xml").unwrap();

        let urls = parse_feeds_file(file.path()).await.unwrap();
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://example.com/feed.xml");
        assert_eq!(urls[1], "https://example.org/rss");
        assert_eq!(urls[2], "https://example.net/atom.xml");
    }
}
