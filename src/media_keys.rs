//! Media keys and MPRIS (the desktop's media controls), on a worker thread.
//!
//! `souvlaki` talks to the session bus; that work stays off the UI thread. The UI sends
//! [`Status`] updates in and polls [`Command`]s out; neither call blocks. If the bus is
//! unavailable the worker exits quietly and Lumen runs without desktop media controls.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};

/// What the desktop asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

/// Maps a desktop event to a command; events Lumen does not act on map to `None`.
pub fn command_for(event: &MediaControlEvent) -> Option<Command> {
    Some(match event {
        MediaControlEvent::Toggle => Command::Toggle,
        MediaControlEvent::Play => Command::Play,
        MediaControlEvent::Pause => Command::Pause,
        MediaControlEvent::Stop => Command::Stop,
        MediaControlEvent::Next => Command::Next,
        MediaControlEvent::Previous => Command::Previous,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Stopped,
    Paused,
    Playing,
}

/// What the player is doing now, as shown by the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub title: Option<String>,
    pub playback: Playback,
    pub position_ms: u64,
}

impl Status {
    /// Whether the desktop needs to hear about a change. Position alone changes every tick,
    /// so it only counts while paused or stopped; MPRIS clients extrapolate while playing.
    pub fn differs_from(&self, last: &Status) -> bool {
        self.title != last.title
            || self.playback != last.playback
            || (self.playback != Playback::Playing && self.position_ms != last.position_ms)
    }
}

pub struct MediaKeys {
    updates: Sender<Status>,
    commands: Receiver<Command>,
    last: Option<Status>,
}

impl MediaKeys {
    /// Starts the worker. `None` only when the thread cannot be created.
    pub fn spawn() -> Option<Self> {
        let (updates, update_rx) = mpsc::channel::<Status>();
        let (command_tx, commands) = mpsc::channel::<Command>();
        thread::Builder::new()
            .name("media-keys".into())
            .spawn(move || worker(update_rx, command_tx))
            .ok()?;
        Some(Self {
            updates,
            commands,
            last: None,
        })
    }

    /// Commands received since the last call.
    pub fn poll(&self) -> Vec<Command> {
        self.commands.try_iter().collect()
    }

    /// Passes a status to the worker when it changed.
    pub fn publish(&mut self, status: Status) {
        if self
            .last
            .as_ref()
            .is_none_or(|last| status.differs_from(last))
        {
            let _ = self.updates.send(status.clone());
            self.last = Some(status);
        }
    }
}

fn worker(updates: Receiver<Status>, commands: Sender<Command>) {
    let config = PlatformConfig {
        dbus_name: "lumen_rs",
        display_name: "Lumen",
        hwnd: None,
    };
    let Ok(mut controls) = MediaControls::new(config) else {
        return;
    };
    let attached = controls.attach(move |event| {
        if let Some(command) = command_for(&event) {
            let _ = commands.send(command);
        }
    });
    if attached.is_err() {
        return;
    }
    loop {
        match updates.recv_timeout(Duration::from_secs(5)) {
            Ok(status) => {
                let progress = Some(MediaPosition(Duration::from_millis(status.position_ms)));
                let _ = controls.set_metadata(MediaMetadata {
                    title: status.title.as_deref(),
                    ..Default::default()
                });
                let _ = controls.set_playback(match status.playback {
                    Playback::Stopped => MediaPlayback::Stopped,
                    Playback::Paused => MediaPlayback::Paused { progress },
                    Playback::Playing => MediaPlayback::Playing { progress },
                });
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The UI is gone.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(playback: Playback, position_ms: u64) -> Status {
        Status {
            title: Some("a".into()),
            playback,
            position_ms,
        }
    }

    #[test]
    fn desktop_events_map_to_commands() {
        assert_eq!(
            command_for(&MediaControlEvent::Toggle),
            Some(Command::Toggle)
        );
        assert_eq!(command_for(&MediaControlEvent::Next), Some(Command::Next));
        assert_eq!(command_for(&MediaControlEvent::Raise), None);
    }

    #[test]
    fn position_ticks_while_playing_are_not_republished() {
        let playing = status(Playback::Playing, 1000);
        assert!(!status(Playback::Playing, 2000).differs_from(&playing));
        assert!(status(Playback::Paused, 2000).differs_from(&playing));
        let paused = status(Playback::Paused, 2000);
        assert!(status(Playback::Paused, 3000).differs_from(&paused));
    }
}
