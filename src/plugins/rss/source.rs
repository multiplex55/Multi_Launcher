use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::{Client, Response};
use scraper::{Html, Selector};
use serde_json::Value;
use url::Url;

/// Type of feed returned by the resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedType {
    Atom,
    Rss,
    Json,
}

/// Type of source that produced the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    YoutubeChannel,
    YoutubePlaylist,
    Generic,
}

/// Metadata returned when resolving a feed source.
#[derive(Debug, Clone)]
pub struct ResolvedSource {
    pub feed_url: String,
    pub site_url: Option<String>,
    pub feed_type: FeedType,
    pub source_type: SourceType,
}

/// Resolve a user provided source string to a concrete feed.
///
/// The resolver handles a number of common cases:
/// * `@handle` – YouTube channel handle which is resolved to a channel id
///   and turned into a feed URL.
/// * YouTube channel / playlist URLs – turned into feed URLs.
/// * Direct feed URLs – simply verified and normalised.
/// * Generic site URLs – feed autodiscovery is attempted.
pub fn resolve(source: &str) -> Result<ResolvedSource> {
    let source = source.trim();
    if source.is_empty() {
        bail!("empty source");
    }

    let client = Client::builder()
        .user_agent("multi-launcher rss resolver")
        .build()?;

    // 1) YouTube handle starting with '@'.
    if let Some(handle) = source.strip_prefix('@') {
        return resolve_youtube_handle(&client, handle);
    }

    // Try to parse as URL; prepend https:// if no scheme.
    let url = match Url::parse(source) {
        Ok(u) => u,
        Err(_) => Url::parse(&format!("https://{source}"))?,
    };

    // 2) YouTube URLs for handles/channels/playlists.
    if url
        .host_str()
        .map(|h| h.ends_with("youtube.com") || h == "youtu.be")
        .unwrap_or(false)
    {
        if let Some(handle) = url.path().strip_prefix("/@") {
            return resolve_youtube_handle(&client, handle);
        }
        if let Some(id) = youtube_channel_id_from_url(&url) {
            return resolve_youtube_channel(&client, id);
        }
        if let Some(id) = youtube_playlist_id_from_url(&url) {
            return resolve_youtube_playlist(&client, id);
        }
    }

    // 3) Direct feed URL – attempt to detect feed type.
    if let Ok(feed_type) = detect_feed_type(&client, url.as_str()) {
        let url_str = url.to_string();
        return Ok(ResolvedSource {
            feed_url: url_str.clone(),
            site_url: Some(url_str),
            feed_type,
            source_type: SourceType::Generic,
        });
    }

    // 4) Generic site URL – attempt feed autodiscovery.
    if let Some(feed_url) = discover_feed_url(&client, &url)? {
        let feed_type = detect_feed_type(&client, &feed_url)?;
        return Ok(ResolvedSource {
            feed_url,
            site_url: Some(url.to_string()),
            feed_type,
            source_type: SourceType::Generic,
        });
    }

    bail!("could not resolve feed source");
}

fn resolve_youtube_handle(client: &Client, handle: &str) -> Result<ResolvedSource> {
    if let Ok(api_key) = std::env::var("YOUTUBE_API_KEY") {
        let api_url = format!("https://www.googleapis.com/youtube/v3/channels?part=id&forHandle={handle}&key={api_key}");
        if let Ok(resp) = client.get(&api_url).send() {
            if let Ok(text) = resp.text() {
                if let Ok(json) = serde_json::from_str::<Value>(&text) {
                    if let Some(id) = json["items"]
                        .as_array()
                        .and_then(|a| a.get(0))
                        .and_then(|i| i["id"].as_str())
                    {
                        return resolve_youtube_channel(client, id.to_string());
                    }
                }
            }
        }
    }

    let url = format!("https://www.youtube.com/@{handle}");
    let body = client
        .get(&url)
        .send()
        .context("request handle page")?
        .text()
        .context("read handle page")?;

    let re = Regex::new(r#"channelId":"([A-Za-z0-9_-]+)"#).unwrap();
    let channel_id = re
        .captures(&body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        .ok_or_else(|| anyhow::anyhow!("channel ID not found"))?;
    resolve_youtube_channel(client, channel_id)
}

fn resolve_youtube_channel(_client: &Client, channel_id: String) -> Result<ResolvedSource> {
    let feed_url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
    Ok(ResolvedSource {
        feed_url,
        site_url: Some(format!("https://www.youtube.com/channel/{channel_id}")),
        feed_type: FeedType::Atom,
        source_type: SourceType::YoutubeChannel,
    })
}

fn resolve_youtube_playlist(_client: &Client, playlist_id: String) -> Result<ResolvedSource> {
    let feed_url = format!("https://www.youtube.com/feeds/videos.xml?playlist_id={playlist_id}");
    Ok(ResolvedSource {
        feed_url,
        site_url: Some(format!(
            "https://www.youtube.com/playlist?list={playlist_id}"
        )),
        feed_type: FeedType::Atom,
        source_type: SourceType::YoutubePlaylist,
    })
}

fn youtube_channel_id_from_url(url: &Url) -> Option<String> {
    // https://www.youtube.com/channel/<id>
    if let Some(id) = url.path().strip_prefix("/channel/") {
        return Some(id.trim_end_matches('/').to_string());
    }
    // Direct feed URL: https://www.youtube.com/feeds/videos.xml?channel_id=<id>
    if url.path() == "/feeds/videos.xml" {
        if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "channel_id") {
            return Some(id.into_owned());
        }
    }
    None
}

fn youtube_playlist_id_from_url(url: &Url) -> Option<String> {
    if url.path() == "/playlist" {
        if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "list") {
            return Some(id.into_owned());
        }
    }
    if url.path() == "/feeds/videos.xml" {
        if let Some((_, id)) = url.query_pairs().find(|(k, _)| k == "playlist_id") {
            return Some(id.into_owned());
        }
    }
    None
}

fn detect_feed_type(client: &Client, url: &str) -> Result<FeedType> {
    let resp = client.get(url).send().context("fetch feed")?;
    parse_feed_response(resp)
}

fn parse_feed_response(resp: Response) -> Result<FeedType> {
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let body = resp.text().context("read feed body")?;
    if content_type.contains("application/feed+json") || content_type.contains("json") {
        if serde_json::from_str::<Value>(&body).is_ok() {
            return Ok(FeedType::Json);
        }
    }
    let trimmed = body.trim_start();
    if trimmed.starts_with('<') {
        if trimmed.starts_with("<feed") {
            return Ok(FeedType::Atom);
        }
        if trimmed.starts_with("<rss") {
            return Ok(FeedType::Rss);
        }
    }
    // Fallback: basic detection using presence of root tags
    if trimmed.contains("<rss") {
        return Ok(FeedType::Rss);
    }
    if trimmed.contains("<feed") {
        return Ok(FeedType::Atom);
    }
    if trimmed.starts_with('{') && serde_json::from_str::<Value>(trimmed).is_ok() {
        return Ok(FeedType::Json);
    }
    bail!("unknown feed format");
}

fn discover_feed_url(client: &Client, url: &Url) -> Result<Option<String>> {
    let resp = client.get(url.clone()).send().context("fetch page")?;
    let base_url = resp.url().clone();
    let body = resp.text().context("read page")?;
    let document = Html::parse_document(&body);
    let selector = Selector::parse("link[rel=\"alternate\"]").unwrap();
    for elem in document.select(&selector) {
        let ty = elem.value().attr("type").unwrap_or("");
        if !(ty.contains("rss") || ty.contains("atom") || ty.contains("json")) {
            continue;
        }
        if let Some(href) = elem.value().attr("href") {
            if let Ok(feed_url) = base_url.join(href) {
                return Ok(Some(feed_url.to_string()));
            }
        }
    }
    Ok(None)
}
