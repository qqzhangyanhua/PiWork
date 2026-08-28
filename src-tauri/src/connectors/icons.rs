use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use image::ImageFormat;
use reqwest::{Client, Url, redirect::Policy};
use tauri::{AppHandle, Manager};

const CACHE_DIRECTORY: &str = "connector-icons";
const MAX_ICON_BYTES: usize = 512 * 1024;
const MAX_HTML_BYTES: usize = 256 * 1024;
const MAX_DISCOVERED_ICONS: usize = 8;

pub async fn resolve(
    app: &AppHandle,
    service: &str,
    homepage_url: Option<&str>,
    icon_url: Option<&str>,
) -> Option<String> {
    if !valid_service(service) {
        return None;
    }
    let cache_directory = app.path().app_cache_dir().ok()?.join(CACHE_DIRECTORY);
    let cache_path = cache_directory.join(format!("{service}.png"));
    if let Some(data_url) = read_cached_icon(&cache_path).await {
        return Some(data_url);
    }

    let homepage = homepage_url.and_then(safe_https_url);
    let explicit_icon = icon_url.and_then(safe_https_url);
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .redirect(Policy::limited(4))
        .user_agent("CoDo Connector Catalog/0.1")
        .build()
        .ok()?;

    let mut candidates = Vec::new();
    if let Some(url) = explicit_icon {
        candidates.push(url);
    }
    if let Some(url) = homepage.as_ref().and_then(favicon_at_origin) {
        candidates.push(url);
    }

    for candidate in candidates {
        if let Some(png) = download_png(&client, candidate).await {
            return cache_and_encode(&cache_directory, &cache_path, png).await;
        }
    }

    let homepage = homepage?;
    let (_, html) = fetch_limited(&client, homepage.clone(), MAX_HTML_BYTES).await?;
    let html = String::from_utf8_lossy(&html);
    for candidate in extract_icon_urls(&html, &homepage)
        .into_iter()
        .take(MAX_DISCOVERED_ICONS)
    {
        if let Some(png) = download_png(&client, candidate).await {
            return cache_and_encode(&cache_directory, &cache_path, png).await;
        }
    }
    None
}

async fn read_cached_icon(path: &Path) -> Option<String> {
    let bytes = tokio::fs::read(path).await.ok()?;
    (!bytes.is_empty()).then(|| png_data_url(&bytes))
}

async fn cache_and_encode(directory: &Path, path: &Path, png: Vec<u8>) -> Option<String> {
    tokio::fs::create_dir_all(directory).await.ok()?;
    tokio::fs::write(path, &png).await.ok()?;
    Some(png_data_url(&png))
}

fn png_data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}

async fn download_png(client: &Client, url: Url) -> Option<Vec<u8>> {
    let (_, bytes) = fetch_limited(client, url, MAX_ICON_BYTES).await?;
    normalize_to_png(&bytes)
}

async fn fetch_limited(client: &Client, url: Url, limit: usize) -> Option<(Url, Vec<u8>)> {
    let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return None;
    }
    let final_url = response.url().clone();
    safe_https_url(final_url.as_str())?;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some((final_url, bytes))
}

fn normalize_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?.thumbnail(96, 96);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).ok()?;
    Some(output.into_inner())
}

fn valid_service(service: &str) -> bool {
    !service.is_empty()
        && service.len() <= 80
        && service.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn safe_https_url(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if let Ok(address) = host.parse::<IpAddr>() {
        match address {
            IpAddr::V4(address) if unsafe_ipv4(address) => return None,
            IpAddr::V6(address) if unsafe_ipv6(address) => return None,
            _ => {}
        }
    } else if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return None;
    }
    Some(url)
}

fn unsafe_ipv4(address: Ipv4Addr) -> bool {
    address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_unspecified()
        || address.is_multicast()
}

fn unsafe_ipv6(address: Ipv6Addr) -> bool {
    address.is_loopback()
        || address.is_unspecified()
        || address.is_unique_local()
        || address.is_unicast_link_local()
        || address.is_multicast()
        || matches!(address.to_ipv4_mapped(), Some(ipv4) if unsafe_ipv4(ipv4))
}

fn favicon_at_origin(homepage: &Url) -> Option<Url> {
    let mut favicon = homepage.clone();
    favicon.set_path("/favicon.ico");
    favicon.set_query(None);
    favicon.set_fragment(None);
    safe_https_url(favicon.as_str())
}

fn extract_icon_urls(html: &str, base: &Url) -> Vec<Url> {
    let lowercase = html.to_ascii_lowercase();
    let mut urls = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = lowercase[offset..].find("<link") {
        let start = offset + relative_start;
        let Some(relative_end) = lowercase[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        let tag = &html[start..end];
        let rel = html_attribute(tag, "rel").unwrap_or_default();
        let is_icon = rel.split_ascii_whitespace().any(|value| {
            value.eq_ignore_ascii_case("icon") || value.eq_ignore_ascii_case("shortcut")
        });
        if is_icon
            && let Some(href) = html_attribute(tag, "href")
            && let Ok(url) = base.join(&href)
            && let Some(url) = safe_https_url(url.as_str())
            && !urls.contains(&url)
        {
            urls.push(url);
        }
        offset = end;
    }
    urls
}

fn html_attribute(tag: &str, attribute: &str) -> Option<String> {
    let lowercase = tag.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut offset = 0;
    while let Some(relative_start) = lowercase[offset..].find(attribute) {
        let start = offset + relative_start;
        let before_is_boundary =
            start == 0 || !lowercase.as_bytes()[start - 1].is_ascii_alphanumeric();
        let mut cursor = start + attribute.len();
        let after_is_boundary = cursor >= bytes.len() || !bytes[cursor].is_ascii_alphanumeric();
        if !before_is_boundary || !after_is_boundary {
            offset = cursor;
            continue;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            offset = cursor;
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let quote = bytes.get(cursor).copied();
        if matches!(quote, Some(b'\'' | b'"')) {
            cursor += 1;
            let end = bytes[cursor..]
                .iter()
                .position(|byte| Some(*byte) == quote)?
                + cursor;
            return Some(tag[cursor..end].trim().to_owned());
        }
        let end = bytes[cursor..]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'>')
            .map(|length| cursor + length)
            .unwrap_or(bytes.len());
        return Some(tag[cursor..end].trim().to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgba};

    use super::*;

    #[test]
    fn rejects_local_and_non_https_icon_sources() {
        assert!(safe_https_url("http://example.com/favicon.ico").is_none());
        assert!(safe_https_url("https://localhost/favicon.ico").is_none());
        assert!(safe_https_url("https://127.0.0.1/favicon.ico").is_none());
        assert!(safe_https_url("https://10.0.0.8/favicon.ico").is_none());
        assert!(safe_https_url("https://example.com/favicon.ico").is_some());
    }

    #[test]
    fn discovers_relative_icon_links() {
        let base = Url::parse("https://example.com/products/connectors").unwrap();
        let urls = extract_icon_urls(
            r#"<link rel="stylesheet" href="/site.css"><link href='/assets/icon.png' rel='shortcut icon'>"#,
            &base,
        );

        assert_eq!(
            urls,
            vec![Url::parse("https://example.com/assets/icon.png").unwrap()]
        );
    }

    #[test]
    fn normalizes_downloaded_images_to_png() {
        let source =
            DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(8, 8, Rgba([40, 90, 220, 255])));
        let mut encoded = Cursor::new(Vec::new());
        source.write_to(&mut encoded, ImageFormat::Png).unwrap();

        let normalized = normalize_to_png(&encoded.into_inner()).unwrap();
        assert_eq!(&normalized[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn validates_cache_keys() {
        assert!(valid_service("google_drive"));
        assert!(valid_service("17track"));
        assert!(!valid_service("../escape"));
        assert!(!valid_service("Gmail"));
    }
}
