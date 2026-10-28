//! Integration tests for streams: Streamlink's relay, yt-dlp resolution and SponsorBlock skipping.
//!
//! Nothing here needs the internet. HLS is served from a local HTTP server, and the yt-dlp and
//! Streamlink failure cases use small fake programs. The relay test uses the real `streamlink`
//! and is skipped when it is not installed.

// The fake yt-dlp/Streamlink programs are POSIX shell scripts, so these tests run on Unix only.
#![cfg(unix)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;

use common::{fixture, play, session, wait};
use lumen_rs::engine::TrackKind;
use lumen_rs::session::{Session, State};
use lumen_rs::sponsorblock::Segment;
use lumen_rs::streams::{Plan, Start, Tools};

/// Serves `dir` over HTTP on 127.0.0.1 until the test process ends. Returns the port.
fn serve(dir: PathBuf) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let dir = dir.clone();
            thread::spawn(move || respond(stream, &dir));
        }
    });
    port
}

fn respond(mut stream: std::net::TcpStream, dir: &Path) {
    let mut buffer = [0u8; 4096];
    let n = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
    let name = request
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .split('?')
        .next()
        .unwrap_or("");
    let body = (!name.contains(".."))
        .then(|| std::fs::read(dir.join(name)).ok())
        .flatten();
    let kind = match name.rsplit('.').next().unwrap_or("") {
        "m3u8" => "application/vnd.apple.mpegurl",
        "mp4" => "video/mp4",
        "m4a" => "audio/mp4",
        _ => "video/mp2t",
    };
    // mpv seeks inside MP4 files, so honour a `Range: bytes=start-[end]` header.
    let range = request.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let spec = key
            .eq_ignore_ascii_case("range")
            .then(|| value.trim().strip_prefix("bytes="))??;
        let (start, end) = spec.split_once('-')?;
        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()))
    });
    let _ = match body {
        Some(body) => {
            let (status, slice, extra) = match range {
                Some((start, end)) if start < body.len() => {
                    let end = end.map_or(body.len() - 1, |e| e.min(body.len() - 1));
                    (
                        "206 Partial Content",
                        &body[start..=end],
                        format!("Content-Range: bytes {start}-{end}/{}\r\n", body.len()),
                    )
                }
                _ => ("200 OK", &body[..], String::new()),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nAccept-Ranges: bytes\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
                slice.len()
            );
            stream
                .write_all(head.as_bytes())
                .and_then(|_| stream.write_all(slice))
        }
        None => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    };
}

/// Writes an executable shell script and returns its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The URL of `name` under `fx` on a local server. yt-dlp output must be http(s), so file URLs won't do.
fn http_url(fx: &common::Fixture, name: &str) -> String {
    let port = serve(fx.root.clone());
    format!("http://127.0.0.1:{port}/{name}")
}

/// A session with its own Streamlink and yt-dlp, and SponsorBlock off so nothing touches the network.
fn session_with(tools: Tools) -> Session {
    let mut s = session();
    s.store.settings.sponsorblock = false;
    s.streams.tools = tools;
    s
}

fn real_streamlink() -> Option<PathBuf> {
    Tools::detect().streamlink
}

#[test]
fn streamlink_relays_a_local_hls_stream_and_switches_quality() {
    let Some(fx) = fixture() else { return };
    let Some(streamlink) = real_streamlink() else {
        eprintln!("skipped: streamlink is not installed");
        return;
    };
    let port = serve(fx.root.join("hls"));
    let page = format!("http://127.0.0.1:{port}/index.m3u8");
    let mut s = session_with(Tools {
        streamlink: Some(streamlink),
        ytdlp: None,
    });
    s.streams.force(Some(Start::Streamlink));

    s.add(std::slice::from_ref(&page), true);
    assert!(
        s.status().contains("OPENING LINK") || s.status().contains("STREAMLINK"),
        "{}",
        s.status()
    );
    assert!(
        wait(&mut s, 25, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    let relay = s.engine.path().unwrap();
    assert!(
        relay.starts_with("http://127.0.0.1:"),
        "plays through the local relay, not {relay}"
    );
    assert_ne!(relay, page);

    assert!(
        wait(&mut s, 15, |s| !s.stream_qualities().is_empty()),
        "Streamlink lists its qualities"
    );
    assert_eq!(s.stream_qualities(), ["best", "live"]);
    assert_eq!(s.stream_quality(), "best");

    s.set_stream_quality("worst");
    assert_eq!(s.stream_quality(), "worst");
    assert!(
        wait(&mut s, 25, |s| s.state() == State::Playing
            && s.engine.path().is_some_and(|p| p != relay)),
        "restart at the new quality"
    );

    assert!(
        s.is_live(),
        "a relayed stream is live even though mpv reports a buffer length"
    );
    s.stop();
    assert_eq!(s.streams.live_processes(), 0, "stopping ends the relay");
    assert_eq!(
        s.store.file_state(&page).position,
        0,
        "a live stream has no position worth resuming"
    );
}

#[test]
fn media_links_play_directly_without_streamlink() {
    let Some(fx) = fixture() else { return };
    let port = serve(fx.root.join("hls"));
    let page = format!("http://127.0.0.1:{port}/index.m3u8");
    let mut s = session_with(Tools::detect());
    assert_eq!(s.streams.plan(&page), Plan::Direct);
    s.add(std::slice::from_ref(&page), true);
    assert!(
        wait(&mut s, 15, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    assert_eq!(s.engine.path().as_deref(), Some(page.as_str()));
    assert_eq!(s.streams.live_processes(), 0);
}

#[test]
fn ytdlp_resolves_separate_video_and_audio_streams() {
    let Some(fx) = fixture() else { return };
    let log = fx.path("ytdlp-args.txt");
    let video = http_url(&fx, "video_only.mp4");
    let audio = http_url(&fx, "audio_only.m4a");
    let ytdlp = script(
        &fx.root,
        "fake-ytdlp",
        &format!(
            "printf '%s\\n' \"$@\" > '{log}'\necho 'Generic abc123'\necho 'A Resolved Title'\necho '{video}'\necho '{audio}'"
        ),
    );
    let mut s = session_with(Tools {
        streamlink: None,
        ytdlp: Some(ytdlp),
    });
    s.streams.force(Some(Start::Ytdlp));

    s.add(&["https://video.test/watch?v=abc123".to_owned()], true);
    assert!(
        s.status().contains("OPENING LINK") || s.status().contains("YT-DLP"),
        "{}",
        s.status()
    );
    assert!(
        wait(&mut s, 15, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );

    assert_eq!(
        s.queue.items[0].title, "A Resolved Title",
        "the queue shows the video's title"
    );
    assert_eq!(s.engine.path().as_deref(), Some(video.as_str()));
    let audio_tracks = s.engine.tracks(TrackKind::Audio);
    assert_eq!(
        audio_tracks.len(),
        1,
        "the separate audio stream is attached"
    );

    let args = std::fs::read_to_string(&log).unwrap();
    for expected in [
        "--no-playlist",
        "-f",
        "b/bv*+ba",
        "--",
        "https://video.test/watch?v=abc123",
    ] {
        assert!(
            args.lines().any(|l| l == expected),
            "yt-dlp was not given {expected}: {args}"
        );
    }
}

#[test]
fn ytdlp_failure_falls_back_to_the_link_itself() {
    let Some(fx) = fixture() else { return };
    let ytdlp = script(
        &fx.root,
        "failing-ytdlp",
        "echo 'WARNING: noise' >&2\necho 'ERROR: Unsupported URL: x' >&2\nexit 1",
    );
    let mut s = session_with(Tools {
        streamlink: None,
        ytdlp: Some(ytdlp),
    });
    s.streams.force(Some(Start::Ytdlp));

    let page = fx.path("single/film.mp4"); // mpv can open it directly
    s.add(std::slice::from_ref(&page), true);
    assert!(
        wait(&mut s, 15, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    assert_eq!(s.engine.path().as_deref(), Some(page.as_str()));
    assert!(
        wait(&mut s, 1, |s| s.status().contains("trying it directly"))
            || s.state() == State::Playing
    );
}

#[test]
fn an_offline_channel_is_final_and_yt_dlp_is_not_tried() {
    let Some(fx) = fixture() else { return };
    let marker = fx.path("ytdlp-was-called");
    let streamlink = script(
        &fx.root,
        "offline-streamlink",
        "echo 'error: No playable streams found on this URL: x' >&2\nexit 1",
    );
    let ytdlp = script(
        &fx.root,
        "marking-ytdlp",
        &format!("touch '{marker}'\nexit 1"),
    );
    let mut s = session_with(Tools {
        streamlink: Some(streamlink),
        ytdlp: Some(ytdlp),
    });
    s.streams.force(Some(Start::Streamlink));

    s.add(&["https://www.twitch.tv/offline".to_owned()], true);
    assert!(
        wait(&mut s, 10, |s| s
            .status()
            .contains("Streamlink: No playable streams")),
        "{}",
        s.status()
    );
    assert_eq!(s.state(), State::Idle, "nothing plays");
    assert!(
        !Path::new(&marker).exists(),
        "yt-dlp must not be tried for an offline channel"
    );
}

#[test]
fn other_streamlink_failures_fall_back_to_ytdlp() {
    let Some(fx) = fixture() else { return };
    let streamlink = script(
        &fx.root,
        "broken-streamlink",
        "echo 'error: Unable to open URL: x' >&2\nexit 1",
    );
    let video = http_url(&fx, "single/film.mp4");
    let ytdlp = script(
        &fx.root,
        "good-ytdlp",
        &format!("echo 'Generic z'\necho 'From yt-dlp'\necho '{video}'"),
    );
    let mut s = session_with(Tools {
        streamlink: Some(streamlink),
        ytdlp: Some(ytdlp),
    });
    s.streams.force(Some(Start::Streamlink));

    s.add(&["https://live.test/channel".to_owned()], true);
    assert!(
        wait(&mut s, 15, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    assert_eq!(s.queue.items[0].title, "From yt-dlp");
}

#[test]
fn a_slow_lookup_does_not_override_what_the_user_chose_next() {
    let Some(fx) = fixture() else { return };
    let slow_video = http_url(&fx, "single/film.mp4");
    let ytdlp = script(
        &fx.root,
        "slow-ytdlp",
        &format!("sleep 2\necho 'Generic s'\necho 'Slow'\necho '{slow_video}'"),
    );
    let mut s = session_with(Tools {
        streamlink: None,
        ytdlp: Some(ytdlp),
    });
    s.streams.force(Some(Start::Ytdlp));

    s.add(&["https://slow.test/v".to_owned()], true);
    s.streams.force(None);
    let local = fx.path("show/ep1.mp4");
    s.add(std::slice::from_ref(&local), true);
    assert!(wait(&mut s, 10, |s| s.state() == State::Playing
        && s.position_ms() > 0));
    // Give the slow lookup time to finish, had it not been cancelled.
    assert!(
        !wait(&mut s, 3, |s| s.engine.path().as_deref()
            != Some(local.as_str())),
        "a replaced lookup must be ignored"
    );
    assert_eq!(
        s.queue.items[0].title, "v",
        "the replaced lookup must not rename its entry"
    );
}

#[test]
fn sponsor_segments_are_skipped_once() {
    let Some(fx) = fixture() else { return };
    let mut s = session_with(Tools::default());
    play(&mut s, &fx.path("show/ep1.mp4"));
    let segment = Segment {
        start: 3_000,
        end: 6_000,
        category: "sponsor".to_owned(),
    };
    s.set_sponsor_segments(vec![segment]);
    s.seek_ms(2_500);
    assert!(
        wait(&mut s, 5, |s| s.position_ms() >= 6_000),
        "stopped at {} ms",
        s.position_ms()
    );
    assert!(
        s.status().contains("Skipped sponsor") || s.status().contains("SKIPPED SPONSOR"),
        "{}",
        s.status()
    );

    s.seek_ms(3_500); // seeking back into a skipped segment is respected
    assert!(
        wait(&mut s, 3, |s| (3_000..5_000).contains(&s.position_ms())),
        "the seek did not land"
    );
    assert!(
        !wait(&mut s, 1, |s| s.position_ms() >= 6_000),
        "skipped the same segment twice"
    );
    assert_eq!(
        s.sponsor_categories(),
        ["sponsor", "selfpromo", "interaction"]
    );
    s.store.settings.sponsorblock_intros = true;
    assert_eq!(
        s.sponsor_categories(),
        ["sponsor", "selfpromo", "interaction", "intro", "outro"]
    );
}

#[test]
fn a_segment_nearly_over_is_not_worth_skipping() {
    let Some(fx) = fixture() else { return };
    let mut s = session_with(Tools::default());
    play(&mut s, &fx.path("show/ep1.mp4"));
    s.set_sponsor_segments(vec![Segment {
        start: 3_000,
        end: 6_000,
        category: "sponsor".to_owned(),
    }]);
    s.seek_ms(5_700); // 300 ms before the end
    assert!(wait(&mut s, 3, |s| s.position_ms() >= 5_700));
    assert!(
        s.position_ms() < 6_000 + 600,
        "should have played on, not jumped: {}",
        s.position_ms()
    );
}

fn network_tests() -> bool {
    let on = std::env::var_os("LUMEN_NETWORK_TESTS").is_some();
    if !on {
        eprintln!("skipped: set LUMEN_NETWORK_TESTS=1 to test against real sites");
    }
    on
}

#[test]
fn network_ytdlp_plays_a_youtube_link() {
    let Some(_fx) = fixture() else { return };
    if !network_tests() {
        return;
    }
    let mut s = session();
    s.add(
        &["https://www.youtube.com/watch?v=jNQXAC9IVRw".to_owned()],
        true,
    );
    assert!(
        wait(&mut s, 60, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    assert_eq!(s.queue.items[0].title, "Me at the zoo");
}

#[test]
fn network_streamlink_plays_a_twitch_channel() {
    let Some(_fx) = fixture() else { return };
    if !network_tests() {
        return;
    }
    let mut s = session();
    s.add(&["https://www.twitch.tv/lofigirl".to_owned()], true);
    assert!(
        wait(&mut s, 60, |s| s.state() == State::Playing
            && s.position_ms() > 0),
        "{}",
        s.status()
    );
    assert!(
        s.engine
            .path()
            .is_some_and(|p| p.starts_with("http://127.0.0.1:")),
        "plays through the relay"
    );
    assert!(
        wait(&mut s, 20, |s| !s.stream_qualities().is_empty()),
        "qualities were not listed"
    );
    assert!(
        s.queue.items[0].title.to_lowercase().contains("lofi"),
        "{}",
        s.queue.items[0].title
    );
}
