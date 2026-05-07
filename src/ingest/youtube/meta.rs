/// Metadata parsed from a yt-dlp `.info.json` file.
pub(super) struct YoutubeVideoMeta {
    pub(super) title: String,
    pub(super) video_id: String,
    pub(super) thumbnail: String,
    pub(super) channel: String,
    pub(super) channel_url: String,
    pub(super) uploader_id: String,
    pub(super) upload_date: String,
    pub(super) description: String,
    pub(super) duration_string: String,
    pub(super) view_count: Option<u64>,
    pub(super) like_count: Option<u64>,
    pub(super) tags: Vec<String>,
    pub(super) categories: Vec<String>,
}

/// Parse a yt-dlp `.info.json` file into `YoutubeVideoMeta`. Returns `None` on any parse failure.
pub(super) async fn parse_youtube_info_json(path: &std::path::Path) -> Option<YoutubeVideoMeta> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let svec = |k: &str| -> Vec<String> {
        v[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(YoutubeVideoMeta {
        title: s("title"),
        video_id: s("id"),
        thumbnail: s("thumbnail"),
        channel: s("channel"),
        channel_url: s("channel_url"),
        uploader_id: s("uploader_id"),
        upload_date: s("upload_date"),
        description: s("description"),
        duration_string: s("duration_string"),
        view_count: v["view_count"].as_u64(),
        like_count: v["like_count"].as_u64(),
        tags: svec("tags"),
        categories: svec("categories"),
    })
}

/// Build the YouTube-specific Qdrant payload fields from parsed metadata.
pub(super) fn build_youtube_extra_payload(m: &YoutubeVideoMeta) -> serde_json::Value {
    serde_json::json!({
        "yt_video_id": m.video_id,
        "yt_thumbnail": m.thumbnail,
        "yt_channel": m.channel,
        "yt_channel_url": m.channel_url,
        "yt_uploader_id": m.uploader_id,
        "yt_upload_date": m.upload_date,
        "yt_duration": m.duration_string,
        "yt_view_count": m.view_count,
        "yt_like_count": m.like_count,
        "yt_tags": m.tags,
        "yt_categories": m.categories,
    })
}
