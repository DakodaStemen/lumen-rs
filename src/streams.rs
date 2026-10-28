//! Live streams through Streamlink and video pages through yt-dlp.
//!
//! Both run as subprocesses, never as libraries. Streamlink only strips Twitch's embedded ad
//! segments when it reads the stream itself, so Lumen runs it as a private HTTP server on
//! 127.0.0.1 and points mpv at that instead of at the site's own URL.
//!
//! Each play request becomes one job on a worker thread. Results arrive as [`Event`]s that
//! `Streams::poll` hands to the session; events from a job that has since been replaced are dropped.

use std::io::{self, BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::queue::MEDIA_EXT;
use crate::sponsorblock::Segment;

const POLL: Duration = Duration::from_millis(50);

/// What the user is waiting for while a link is being opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Checking,
    Streamlink,
    Ytdlp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Streamlink is serving the stream at `url`; play that, without resuming.
    Ready {
        page: String,
        url: String,
    },
    /// Title and the quality names Streamlink offers, worst to best.
    Info {
        page: String,
        title: String,
        qualities: Vec<String>,
    },
    /// yt-dlp found the real streams behind a page. `audio` is set when video and audio are separate.
    Resolved {
        page: String,
        extractor: String,
        id: String,
        title: String,
        video: String,
        audio: Option<String>,
    },
    /// Neither tool took the link; play it directly, after showing `notice` if there is one.
    Direct {
        page: String,
        notice: Option<String>,
    },
    /// Nothing can play; show `message`.
    Failed {
        page: String,
        message: String,
    },
    Sponsor {
        video_id: String,
        segments: Vec<Segment>,
    },
    Stage(Activity),
}

/// Which external programs are available.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub streamlink: Option<PathBuf>,
    pub ytdlp: Option<PathBuf>,
}

impl Tools {
    pub fn detect() -> Self {
        Self {
            streamlink: which("streamlink"),
            ytdlp: which("yt-dlp"),
        }
    }
}

fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Where a job starts. `Auto` decides by asking Streamlink whether a site plugin matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Auto,
    Streamlink,
    Ytdlp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Hand the source straight to mpv.
    Direct,
    /// Let a worker work out the real stream first.
    Resolve,
}

// ----- routing -----

/// Whether `text` is a link Lumen can open from the network dialog.
pub fn is_network_url(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    ["http", "https", "rtsp", "rtmp", "udp", "rtp"]
        .iter()
        .any(|scheme| {
            lower.strip_prefix(scheme).is_some_and(|rest| {
                rest.strip_prefix("://")
                    .is_some_and(|host| !host.is_empty())
            })
        })
}

pub fn is_web(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn path_of(url: &str) -> &str {
    let rest = url.split(['?', '#']).next().unwrap_or(url);
    rest.split_once("://")
        .map_or(rest, |(_, r)| r.find('/').map_or("", |i| &r[i..]))
}

/// Plain media files and playlists play fine in mpv directly.
pub fn has_media_extension(url: &str) -> bool {
    let path = path_of(url);
    let name = path.rsplit('/').next().unwrap_or("");
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        MEDIA_EXT.contains(&ext.to_lowercase().as_str())
            || matches!(ext.to_lowercase().as_str(), "m3u8" | "mpd")
    })
}

/// YouTube is yt-dlp's job; Streamlink's own YouTube support is not used.
pub fn is_youtube(url: &str) -> bool {
    let host = url
        .split_once("://")
        .map_or("", |(_, r)| r.split(['/', '?', '#']).next().unwrap_or(""));
    let host = host
        .rsplit('@')
        .next()
        .unwrap_or(host)
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    ["youtube.com", "youtu.be", "youtube-nocookie.com"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

// ----- parsing tool output -----

/// What yt-dlp printed for a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub extractor: String,
    pub id: String,
    pub title: String,
    pub urls: Vec<String>,
}

/// Whether `quality` is safe to put in Streamlink's stream argument: letters, digits, `_` and `+`.
pub fn is_valid_quality(quality: &str) -> bool {
    !quality.is_empty()
        && quality
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'+')
}

/// Reads the output of `--print "%(extractor_key)s %(id)s" --print title --print urls`.
pub fn parse_ytdlp(output: &str) -> Option<Resolved> {
    let mut lines = output.lines();
    let (extractor, id) = lines.next()?.split_once(' ')?;
    let title = lines.next()?.trim().to_owned();
    let urls: Vec<String> = lines
        .map(str::trim)
        .filter(|l| is_web(l))
        .map(str::to_owned)
        .collect();
    Some(Resolved {
        extractor: extractor.to_owned(),
        id: id.to_owned(),
        title,
        urls,
    })
}

/// The last line of a tool's error output, shortened for the status line.
pub fn last_error_line(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| l.chars().take(120).collect())
}

/// The last error in Streamlink's log, without its `[cli][error]` or `error:` prefix.
pub fn streamlink_error(log: &str) -> String {
    log.lines()
        .rfind(|l| l.to_lowercase().contains("error"))
        .map_or("The stream ended.".to_owned(), |line| {
            let message = line.splitn(3, ']').last().unwrap_or(line).trim();
            let has_prefix = message
                .get(..6)
                .is_some_and(|p| p.eq_ignore_ascii_case("error:"));
            if has_prefix {
                message[6..].trim()
            } else {
                message
            }
            .to_owned()
        })
}

/// Title and quality names from `streamlink --json`. `None` when Streamlink reported an error.
pub fn parse_probe(json: &str) -> Option<(String, Vec<String>)> {
    let data: Value = serde_json::from_str(json).ok()?;
    if data.get("error").is_some() {
        return None; // the relay reports the same problem
    }
    let meta = &data["metadata"];
    let title = ["author", "title"]
        .iter()
        .filter_map(|k| meta[*k].as_str())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" — ");
    let qualities = data["streams"]
        .as_object()
        .map(|m| {
            m.keys()
                .filter(|q| !matches!(q.as_str(), "worst" | "best"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    Some((title, qualities))
}

// ----- processes -----

/// The processes of one job, so they can all be stopped together.
#[derive(Clone)]
struct Job {
    id: u64,
    cancelled: Arc<AtomicBool>,
    children: Arc<Mutex<Vec<Arc<Mutex<Child>>>>>,
}

impl Job {
    fn kill(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        for child in self.children.lock().unwrap().drain(..) {
            let mut child = child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn live_processes(&self) -> usize {
        let children = self.children.lock().unwrap();
        children
            .iter()
            .filter(|c| c.lock().unwrap().try_wait().ok().flatten().is_none())
            .count()
    }
}

struct Proc {
    child: Arc<Mutex<Child>>,
    stdout: Box<dyn Read + Send>,
    stderr: Box<dyn Read + Send>,
}

fn spawn(job: &Job, command: &mut Command) -> io::Result<Proc> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = Box::new(child.stdout.take().expect("stdout is piped"));
    let stderr = Box::new(child.stderr.take().expect("stderr is piped"));
    let child = Arc::new(Mutex::new(child));
    job.children.lock().unwrap().push(child.clone());
    Ok(Proc {
        child,
        stdout,
        stderr,
    })
}

/// Waits for a process to exit. `None` if the job was cancelled first.
fn wait_for(child: &Mutex<Child>, job: &Job) -> Option<bool> {
    loop {
        if job.cancelled() {
            return None;
        }
        if let Some(status) = child.lock().unwrap().try_wait().ok().flatten() {
            return Some(status.success());
        }
        thread::sleep(POLL);
    }
}

/// The most output kept from a tool; the rest is read and discarded so the tool never blocks.
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

fn read_text(mut reader: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut reader).take(MAX_OUTPUT).read_to_end(&mut bytes);
        let _ = io::copy(&mut reader, &mut io::sink());
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Sends each line of `reader` down `tx`, replacing invalid UTF-8 rather than stopping.
fn forward_lines(reader: impl Read + Send + 'static, tx: Sender<String>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        while reader.read_until(b'\n', &mut buffer).is_ok_and(|n| n > 0) {
            if tx
                .send(String::from_utf8_lossy(&buffer).trim_end().to_owned())
                .is_err()
            {
                break;
            }
            buffer.clear();
        }
    });
}

fn free_port() -> io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

// ----- the worker -----

struct Pipeline {
    job: Job,
    tx: Sender<(u64, Event)>,
    tools: Tools,
    page: String,
    quality: String,
    low_latency: bool,
    start: Start,
}

impl Pipeline {
    fn emit(&self, event: Event) {
        let _ = self.tx.send((self.job.id, event));
    }

    fn run(self) {
        let streamlink = self
            .tools
            .streamlink
            .clone()
            .filter(|_| self.start != Start::Ytdlp && !is_youtube(&self.page));
        if let Some(streamlink) = streamlink {
            let applies = self.start == Start::Streamlink || self.can_handle(&streamlink);
            if self.job.cancelled() {
                return;
            }
            if applies {
                self.emit(Event::Stage(Activity::Streamlink));
                let Some(message) = self.relay(&streamlink) else {
                    return;
                };
                if self.job.cancelled() {
                    return;
                }
                // "No playable streams" (an offline channel, say) is final; anything else may be a page yt-dlp can read.
                if message.contains("No playable streams") || self.tools.ytdlp.is_none() {
                    self.emit(Event::Failed {
                        page: self.page.clone(),
                        message: format!("Streamlink: {message}"),
                    });
                    return;
                }
            }
        }
        match self.tools.ytdlp.clone() {
            Some(ytdlp) => self.resolve(&ytdlp),
            None => self.emit(Event::Direct {
                page: self.page.clone(),
                notice: None,
            }),
        }
    }

    /// Asks Streamlink whether a site plugin matches. Never touches the network.
    /// Runs as part of the job, so replacing the job stops it.
    fn can_handle(&self, streamlink: &PathBuf) -> bool {
        let mut command = Command::new(streamlink);
        command.args(["--can-handle-url-no-redirect", &self.page]);
        let Ok(Proc {
            child,
            stdout,
            stderr,
        }) = spawn(&self.job, &mut command)
        else {
            return false;
        };
        let (_out, _err) = (read_text(stdout), read_text(stderr));
        wait_for(&child, &self.job).unwrap_or(false)
    }

    /// Runs Streamlink's relay until it exits. Returns the error message if it failed before serving.
    fn relay(&self, streamlink: &PathBuf) -> Option<String> {
        let port = match free_port() {
            Ok(port) => port,
            Err(e) => return Some(format!("no free local port ({e})")),
        };
        let wanted = if self.quality == "best" || !is_valid_quality(&self.quality) {
            "best".to_owned()
        } else {
            format!("{},best", self.quality)
        };
        let mut command = Command::new(streamlink);
        command.args([
            "--player-external-http",
            "--player-external-http-interface",
            "127.0.0.1",
        ]);
        command.args([
            "--player-external-http-port",
            &port.to_string(),
            "--loglevel",
            "info",
        ]);
        if self.low_latency {
            command.args(["--twitch-low-latency", "--kick-low-latency"]);
        }
        command.args(["--", &self.page, &wanted]);
        let proc = match spawn(&self.job, &mut command) {
            Ok(proc) => proc,
            Err(e) => return Some(format!("could not start streamlink ({e})")),
        };
        self.probe(streamlink);

        let (tx, rx) = mpsc::channel();
        forward_lines(proc.stdout, tx.clone());
        forward_lines(proc.stderr, tx);
        let local = format!("http://127.0.0.1:{port}/");
        let (mut log, mut serving) = (String::new(), false);
        loop {
            match rx.recv_timeout(POLL) {
                Ok(line) => {
                    log.push_str(&line);
                    log.push('\n');
                    if !serving && line.contains(&local) {
                        serving = true;
                        self.emit(Event::Ready {
                            page: self.page.clone(),
                            url: local.clone(),
                        });
                    }
                }
                Err(RecvTimeoutError::Timeout) if self.job.cancelled() => return None,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        (!serving && !self.job.cancelled()).then(|| streamlink_error(&log))
    }

    /// Fetches the title and quality list alongside the relay, so it never delays playback.
    fn probe(&self, streamlink: &PathBuf) {
        let mut command = Command::new(streamlink);
        command.args(["--json", "--", &self.page]);
        let Ok(proc) = spawn(&self.job, &mut command) else {
            return;
        };
        let (job, tx, page) = (self.job.clone(), self.tx.clone(), self.page.clone());
        let Proc {
            child,
            stdout,
            stderr,
        } = proc;
        thread::spawn(move || {
            let (out, _err) = (read_text(stdout), read_text(stderr));
            if wait_for(&child, &job).is_none() {
                return;
            }
            if let Some((title, qualities)) = out.join().ok().and_then(|text| parse_probe(&text)) {
                let _ = tx.send((
                    job.id,
                    Event::Info {
                        page,
                        title,
                        qualities,
                    },
                ));
            }
        });
    }

    /// Turns a video page into stream URLs with yt-dlp, falling back to the raw URL.
    fn resolve(&self, ytdlp: &PathBuf) {
        self.emit(Event::Stage(Activity::Ytdlp));
        let mut command = Command::new(ytdlp);
        command.args([
            "--no-warnings",
            "--no-playlist",
            "-f",
            "b/bv*+ba",
            "--print",
            "%(extractor_key)s %(id)s",
        ]);
        command.args(["--print", "title", "--print", "urls", "--", &self.page]);
        let (ok, stdout, stderr) = match spawn(&self.job, &mut command) {
            Ok(Proc {
                child,
                stdout,
                stderr,
            }) => {
                let (out, err) = (read_text(stdout), read_text(stderr));
                let Some(ok) = wait_for(&child, &self.job) else {
                    return;
                };
                (
                    ok,
                    out.join().unwrap_or_default(),
                    err.join().unwrap_or_default(),
                )
            }
            Err(e) => (false, String::new(), e.to_string()),
        };
        if self.job.cancelled() {
            return;
        }
        match parse_ytdlp(&stdout).filter(|r| ok && !r.urls.is_empty()) {
            Some(r) => {
                let mut urls = r.urls.into_iter();
                self.emit(Event::Resolved {
                    page: self.page.clone(),
                    extractor: r.extractor,
                    id: r.id,
                    title: r.title,
                    video: urls.next().expect("checked non-empty"),
                    audio: urls.next(),
                });
            }
            None => {
                let detail = last_error_line(&stderr)
                    .map(|e| format!(" ({e})"))
                    .unwrap_or_default();
                let notice = format!("yt-dlp could not read this link; trying it directly{detail}");
                self.emit(Event::Direct {
                    page: self.page.clone(),
                    notice: Some(notice),
                });
            }
        }
    }
}

// ----- the manager -----

pub struct Streams {
    pub tools: Tools,
    /// Quality chosen from the menu for the current file; falls back to the setting.
    pub quality: Option<String>,
    forced: Option<Start>,
    tx: Sender<(u64, Event)>,
    rx: Receiver<(u64, Event)>,
    job: Option<Job>,
    next_id: u64,
    activity: Option<Activity>,
    page: Option<String>,
    qualities: Vec<String>,
}

impl Streams {
    pub fn new(tools: Tools) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tools,
            quality: None,
            forced: None,
            tx,
            rx,
            job: None,
            next_id: 1,
            activity: None,
            page: None,
            qualities: Vec::new(),
        }
    }

    /// Skips the routing checks and starts every web link at `start`. For tests and tools.
    pub fn force(&mut self, start: Option<Start>) {
        self.forced = start;
    }

    pub fn sender(&self) -> Sender<(u64, Event)> {
        self.tx.clone()
    }

    pub fn plan(&self, source: &str) -> Plan {
        let any_tool = self.tools.streamlink.is_some() || self.tools.ytdlp.is_some();
        if self.forced.is_some() && any_tool {
            Plan::Resolve
        } else if !is_web(source) || has_media_extension(source) || !any_tool {
            Plan::Direct
        } else {
            Plan::Resolve
        }
    }

    pub fn begin(&mut self, page: &str, default_quality: &str, low_latency: bool) {
        self.stop();
        let job = Job {
            id: self.next_id,
            cancelled: Arc::default(),
            children: Arc::default(),
        };
        self.next_id += 1;
        self.activity = Some(Activity::Checking);
        self.page = Some(page.to_owned());
        self.qualities.clear();
        let pipeline = Pipeline {
            job: job.clone(),
            tx: self.tx.clone(),
            tools: self.tools.clone(),
            page: page.to_owned(),
            quality: self
                .quality
                .clone()
                .unwrap_or_else(|| default_quality.to_owned()),
            low_latency,
            start: self.forced.unwrap_or(Start::Auto),
        };
        self.job = Some(job);
        thread::spawn(move || pipeline.run());
    }

    /// Stops the relay and any lookups still running.
    pub fn stop(&mut self) {
        if let Some(job) = self.job.take() {
            job.kill();
        }
        self.activity = None;
    }

    pub fn activity(&self) -> Option<Activity> {
        self.activity
    }

    /// Quality names for `source`, worst to best, once Streamlink has listed them.
    pub fn qualities_for(&self, source: &str) -> &[String] {
        if self.page.as_deref() == Some(source) {
            &self.qualities
        } else {
            &[]
        }
    }

    pub fn live_processes(&self) -> usize {
        self.job.as_ref().map_or(0, Job::live_processes)
    }

    /// Collects what the workers have reported, dropping results from replaced jobs.
    pub fn poll(&mut self) -> Vec<Event> {
        let current = self.job.as_ref().map(|j| j.id);
        let mut events = Vec::new();
        while let Ok((id, event)) = self.rx.try_recv() {
            if id != 0 && Some(id) != current {
                continue;
            }
            match &event {
                Event::Stage(activity) => {
                    self.activity = Some(*activity);
                    continue;
                }
                Event::Ready { .. }
                | Event::Resolved { .. }
                | Event::Direct { .. }
                | Event::Failed { .. } => {
                    self.activity = None;
                }
                Event::Info { qualities, .. } => self.qualities.clone_from(qualities),
                Event::Sponsor { .. } => {}
            }
            events.push(event);
        }
        events
    }
}

impl Drop for Streams {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_network_dialog_accepts_streaming_links_only() {
        for ok in [
            "https://twitch.tv/x",
            " rtsp://cam/stream ",
            "UDP://@:1234",
            "rtmp://live.test/app",
            "rtp://239.0.0.1",
        ] {
            assert!(is_network_url(ok), "{ok}");
        }
        for bad in [
            "",
            "twitch.tv/x",
            "ftp://x",
            "file:///a.mkv",
            "/home/a.mkv",
            "http://",
            "https:/x",
        ] {
            assert!(!is_network_url(bad), "{bad}");
        }
    }

    #[test]
    fn only_http_links_are_routed() {
        assert!(is_web("https://www.twitch.tv/x"));
        assert!(is_web("HTTP://x.test"));
        assert!(!is_web("rtsp://cam"));
        assert!(!is_web("/home/a/ep1.mkv"));
    }

    #[test]
    fn media_files_and_playlists_play_directly() {
        assert!(has_media_extension("https://x.test/a/clip.MP4?token=1"));
        assert!(has_media_extension("https://example.com/live.m3u8"));
        assert!(has_media_extension("https://x.test/manifest.mpd#t=3"));
        assert!(!has_media_extension("https://www.twitch.tv/lofigirl"));
        assert!(!has_media_extension("https://x.test/video.mp4/watch"));
        assert!(
            !has_media_extension("https://host.mp4"),
            "a host name is not a file name"
        );
    }

    #[test]
    fn youtube_goes_to_ytdlp() {
        assert!(is_youtube("https://www.youtube.com/watch?v=jNQXAC9IVRw"));
        assert!(is_youtube("https://youtu.be/jNQXAC9IVRw"));
        assert!(is_youtube("https://m.youtube.com/watch?v=x"));
        assert!(is_youtube("https://www.youtube-nocookie.com/embed/x"));
        assert!(!is_youtube("https://www.twitch.tv/youtube.com"));
        assert!(!is_youtube("https://notyoutube.com/x"));
    }

    #[test]
    fn reads_ytdlp_output() {
        let output =
            "Youtube jNQXAC9IVRw\nMe at the zoo\nhttps://v.test/video\nhttps://v.test/audio\n";
        let r = parse_ytdlp(output).unwrap();
        assert_eq!(
            (r.extractor.as_str(), r.id.as_str(), r.title.as_str()),
            ("Youtube", "jNQXAC9IVRw", "Me at the zoo")
        );
        assert_eq!(r.urls, ["https://v.test/video", "https://v.test/audio"]);

        let untitled = parse_ytdlp("Generic abc\n\nhttps://v.test/only\n").unwrap();
        assert_eq!(untitled.title, "", "an empty title does not shift the URLs");
        assert_eq!(untitled.urls, ["https://v.test/only"]);

        assert!(parse_ytdlp("").is_none());
        assert!(parse_ytdlp("garbage").is_none());
        assert!(
            parse_ytdlp("Youtube x\ntitle\nnot a url\n")
                .unwrap()
                .urls
                .is_empty()
        );
    }

    #[test]
    fn ytdlp_urls_must_be_http() {
        let output = "Generic x\nt\nfile:///etc/passwd\n  HTTPS://v.test/video \nsmb://h/s\nedl://%5\nhttp://v.test/audio\nlavfi://x\n";
        let r = parse_ytdlp(output).unwrap();
        assert_eq!(r.urls, ["HTTPS://v.test/video", "http://v.test/audio"]);
        assert!(
            parse_ytdlp("Generic x\nt\nfile:///etc/passwd\n")
                .unwrap()
                .urls
                .is_empty(),
            "nothing usable left, so the caller falls back"
        );
    }

    #[test]
    fn quality_names_are_validated() {
        for ok in ["best", "720p60", "audio_only", "1080p+", "480p"] {
            assert!(is_valid_quality(ok), "{ok}");
        }
        for bad in [
            "",
            "--player=evil",
            "a,b",
            "a b",
            "-x",
            "720p\n",
            "a/b",
            "é",
        ] {
            assert!(!is_valid_quality(bad), "{bad:?}");
        }
    }

    #[test]
    fn picks_out_the_real_error() {
        assert_eq!(
            last_error_line("warn\nERROR: Video unavailable\n\n").as_deref(),
            Some("ERROR: Video unavailable")
        );
        assert_eq!(last_error_line("   \n"), None);
        assert_eq!(last_error_line(&"x".repeat(300)).unwrap().len(), 120);

        let log = "[cli][info] Found matching plugin twitch\nerror: No playable streams found on this URL: https://twitch.tv/x\n";
        assert_eq!(
            streamlink_error(log),
            "No playable streams found on this URL: https://twitch.tv/x"
        );
        let tagged = "[cli][info] hi\n[cli][error] Unable to open URL\n";
        assert_eq!(streamlink_error(tagged), "Unable to open URL");
        assert_eq!(streamlink_error("[cli][info] bye\n"), "The stream ended.");
    }

    #[test]
    fn reads_the_streamlink_probe_in_quality_order() {
        let json = r#"{"plugin":"twitch","metadata":{"author":"Lofi Girl","title":"beats to relax to"},
            "streams":{"audio_only":{},"360p":{},"720p60":{},"worst":{},"best":{}}}"#;
        let (title, qualities) = parse_probe(json).unwrap();
        assert_eq!(title, "Lofi Girl — beats to relax to");
        assert_eq!(
            qualities,
            ["audio_only", "360p", "720p60"],
            "source order is kept and worst/best are dropped"
        );

        let (title, _) =
            parse_probe(r#"{"metadata":{"author":null,"title":"Only a title"},"streams":{}}"#)
                .unwrap();
        assert_eq!(title, "Only a title");
        assert!(parse_probe(r#"{"error":"No playable streams"}"#).is_none());
        assert!(parse_probe("not json").is_none());
    }

    #[test]
    fn plans_follow_the_tools_available() {
        let mut streams = Streams::new(Tools {
            streamlink: Some("sl".into()),
            ytdlp: None,
        });
        assert_eq!(streams.plan("https://www.twitch.tv/x"), Plan::Resolve);
        assert_eq!(streams.plan("https://x.test/live.m3u8"), Plan::Direct);
        assert_eq!(streams.plan("/home/a/ep1.mkv"), Plan::Direct);
        assert_eq!(streams.plan("rtsp://cam"), Plan::Direct);
        streams.tools = Tools::default();
        assert_eq!(
            streams.plan("https://www.twitch.tv/x"),
            Plan::Direct,
            "without tools mpv tries the link itself"
        );
    }
}
