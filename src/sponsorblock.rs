//! SponsorBlock: community-submitted sponsor segments in YouTube videos, skipped automatically.
//!
//! Lookups use the privacy-preserving hash-prefix endpoint, so the server never learns which
//! video is playing.

use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::streams::Event;

const API: &str = "https://sponsor.ajay.app/api/skipSegments/";

/// A stretch of the video to skip, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Segment {
    pub start: i64,
    pub end: i64,
    pub category: String,
}

pub fn label(category: &str) -> &str {
    match category {
        "selfpromo" => "self-promotion",
        "interaction" => "subscribe reminder",
        other => other,
    }
}

/// The first four hex digits of the video id's SHA-256, which is all the server is told.
pub fn hash_prefix(video_id: &str) -> String {
    Sha256::digest(video_id.as_bytes())
        .iter()
        .take(2)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Picks `video_id`'s skippable segments out of a hash-prefix response, in time order.
pub fn parse_segments(json: &str, video_id: &str) -> Vec<Segment> {
    let Ok(Value::Array(videos)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut found: Vec<Segment> = videos
        .iter()
        .filter(|v| v["videoID"].as_str() == Some(video_id))
        .filter_map(|v| v["segments"].as_array())
        .flatten()
        .filter(|s| s["actionType"].as_str().unwrap_or("skip") == "skip")
        .filter_map(|s| {
            let start = (s["segment"][0].as_f64()? * 1000.0).round() as i64;
            let end = (s["segment"][1].as_f64()? * 1000.0).round() as i64;
            let category = s["category"].as_str().unwrap_or("sponsor").to_owned();
            (end > start).then_some(Segment {
                start,
                end,
                category,
            })
        })
        .collect();
    found.sort_by_key(|a| (a.start, a.end));
    found
}

/// Looks the video up on a background thread and reports through `sink` if anything is found.
pub fn fetch(video_id: String, categories: Vec<String>, sink: Sender<(u64, Event)>) {
    thread::spawn(move || {
        let Some(body) = request(&video_id, &categories) else {
            return;
        };
        let segments = parse_segments(&body, &video_id);
        if !segments.is_empty() {
            let _ = sink.send((0, Event::Sponsor { video_id, segments }));
        }
    });
}

/// A 404 simply means there are no segments for this prefix, so every failure is a quiet `None`.
fn request(video_id: &str, categories: &[String]) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(format!("{API}{}", hash_prefix(video_id)))
        .query("categories", serde_json::to_string(categories).ok()?)
        .query("actionType", "skip")
        .header(
            "User-Agent",
            concat!("Lumen-rs/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .ok()?;
    response.body_mut().read_to_string().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_short_hash_prefix_is_sent() {
        // sha256("jNQXAC9IVRw") starts 0x... ; the prefix is four hex digits.
        let prefix = hash_prefix("jNQXAC9IVRw");
        assert_eq!(prefix.len(), 4);
        assert!(prefix.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(prefix, hash_prefix("jNQXAC9IVRw"));
        assert_ne!(prefix, hash_prefix("dQw4w9WgXcQ"));
    }

    #[test]
    fn keeps_this_videos_skip_segments_in_time_order() {
        let json = r#"[
            {"videoID": "other", "segments": [{"segment": [1, 2], "category": "sponsor"}]},
            {"videoID": "mine", "segments": [
                {"segment": [30.5, 40.25], "category": "selfpromo", "actionType": "skip"},
                {"segment": [5, 9], "category": "sponsor"},
                {"segment": [50, 55], "category": "poi_highlight", "actionType": "poi"},
                {"segment": [60, 60], "category": "sponsor", "actionType": "skip"}
            ]}
        ]"#;
        assert_eq!(
            parse_segments(json, "mine"),
            [
                Segment {
                    start: 5_000,
                    end: 9_000,
                    category: "sponsor".into()
                },
                Segment {
                    start: 30_500,
                    end: 40_250,
                    category: "selfpromo".into()
                },
            ]
        );
    }

    #[test]
    fn bad_responses_give_no_segments() {
        assert!(parse_segments("not json", "x").is_empty());
        assert!(parse_segments("{}", "x").is_empty());
        assert!(
            parse_segments(r#"[{"videoID":"x","segments":[{"segment":["a",2]}]}]"#, "x").is_empty()
        );
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(label("selfpromo"), "self-promotion");
        assert_eq!(label("interaction"), "subscribe reminder");
        assert_eq!(label("intro"), "intro");
    }
}
