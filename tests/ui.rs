//! Interface tests: the real `Frontend` driven through accessible labels, with no window, GPU or
//! audio. A headless libmpv session sits behind it, so playing and pausing really happen.

mod common;

use std::thread::sleep;
use std::time::Duration;

use common::{fixture, session, wait};
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use lumen_rs::keys::Action;
use lumen_rs::session::{Session, State};
use lumen_rs::ui::picker::Picked;
use lumen_rs::ui::{Frontend, NullSurface};

struct Ui {
    session: Option<Session>,
    frontend: Option<Frontend>,
}

impl Ui {
    fn front(&mut self) -> &mut Frontend {
        self.frontend.as_mut().expect("the first frame has run")
    }
}

fn harness(session: Session) -> Harness<'static, Ui> {
    harness_sized(session, [1100.0, 720.0])
}

fn harness_sized(session: Session, size: [f32; 2]) -> Harness<'static, Ui> {
    harness_timed(session, size, 0.25)
}

/// `dt` is how much egui's clock advances per frame; a double-click needs two clicks within 0.3 s.
fn harness_timed(session: Session, size: [f32; 2], dt: f32) -> Harness<'static, Ui> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(dt)
        .build_state(
            |ctx, ui: &mut Ui| {
                if ui.frontend.is_none() {
                    ui.frontend = Some(Frontend::new(ui.session.take().expect("session"), ctx));
                }
                ui.front().show(ctx, &mut NullSurface);
            },
            Ui {
                session: Some(session),
                frontend: None,
            },
        );
    harness.run_steps(3);
    harness
}

/// The first node with this label, for text that appears more than once (a title in the header and the queue).
fn first<'h>(h: &'h Harness<'static, Ui>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_all_by_label(label)
        .next()
        .unwrap_or_else(|| panic!("no node labelled {label:?}"))
}

fn quiet_session() -> Session {
    let mut s = session();
    s.store.settings.sponsorblock = false;
    s
}

/// The player repaints on a timer, so `Harness::run` would never see it go quiet; a few frames do.
fn settle(h: &mut Harness<'static, Ui>) {
    h.run_steps(3);
}

/// Steps the frame loop for `ms` so timers inside the interface fire.
fn pass_time(h: &mut Harness<'static, Ui>, ms: u64) {
    for _ in 0..(ms / 50).max(1) {
        sleep(Duration::from_millis(50));
        h.step();
    }
}

#[test]
fn an_empty_player_invites_you_to_open_something() {
    let Some(_fx) = fixture() else { return };
    let h = harness(quiet_session());
    h.get_by_label("Welcome");
    h.get_by_label("Drop media here");
    h.get_by_label("Open something to begin");
    // One in the sidebar, one over the picture.
    assert_eq!(h.get_all_by_label("+   Open media").count(), 2);
    h.get_by_label("↗   Network stream");
    assert!(h.query_by_label("Nothing queued yet.").is_some());
}

#[test]
fn saving_and_clearing_need_something_in_the_queue() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    assert!(h.get_by_label("Save queue").accesskit_node().is_disabled());
    assert!(h.get_by_label("Clear").accesskit_node().is_disabled());

    h.state_mut()
        .front()
        .session
        .add(&[fx.path("show/ep1.mp4")], false);
    settle(&mut h);
    assert!(!h.get_by_label("Clear").accesskit_node().is_disabled());
    h.get_by_label("Clear").click();
    settle(&mut h);
    assert!(h.state_mut().front().session.queue.items.is_empty());
}

#[test]
fn the_network_dialog_rejects_bad_links_and_opens_good_ones() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.get_by_label("↗   Network stream").click();
    settle(&mut h);
    h.get_by_label("Open network stream");

    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .unwrap();
    field.focus();
    field.type_text("htp://nope");
    settle(&mut h);
    h.get_by_label("Open").click();
    settle(&mut h);
    h.get_by_label("Enter a supported network URL.");
    assert!(h.state_mut().front().session.queue.items.is_empty());

    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .last()
        .unwrap();
    field.focus();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    field.type_text("http://127.0.0.1:9/video.mp4");
    settle(&mut h);
    h.get_by_label("Open").click();
    settle(&mut h);
    let front = h.state_mut().front();
    assert!(
        front.url.is_none(),
        "the dialog closes once the link is accepted"
    );
    assert_eq!(front.session.queue.items.len(), 1);
    assert!(front.session.queue.items[0].source.ends_with("/video.mp4"));
}

#[test]
fn escape_closes_the_network_dialog() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.key_press_modifiers(Modifiers::CTRL, Key::L);
    settle(&mut h);
    h.get_by_label("Open network stream");
    h.key_press(Key::Escape);
    settle(&mut h);
    assert!(h.state_mut().front().url.is_none());
    assert!(h.query_by_label("Open network stream").is_none());
}

#[test]
fn the_queue_filter_narrows_the_list() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    let files = [
        fx.path("show/ep1.mp4"),
        fx.path("show/ep2.mp4"),
        fx.path("single/film.mp4"),
    ];
    h.state_mut().front().session.add(&files, false);
    settle(&mut h);
    first(&h, "ep1.mp4");
    first(&h, "film.mp4");
    h.get_by_label("PLAY QUEUE  ·  3");

    let filter = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap();
    filter.focus();
    filter.type_text("ep");
    settle(&mut h);
    first(&h, "ep1.mp4");
    first(&h, "ep2.mp4");
    assert!(
        h.query_by_label("film.mp4").is_none(),
        "non-matching rows are hidden"
    );

    h.state_mut().front().filter = "zzz".to_owned();
    settle(&mut h);
    h.get_by_label("No matches.");
}

#[test]
fn shuffle_and_repeat_buttons_toggle_the_queue_modes() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    assert!(!h.state_mut().front().session.queue.shuffle);
    h.get_by_label("Shuffle").click();
    h.get_by_label("Repeat").click();
    settle(&mut h);
    let queue = &h.state_mut().front().session.queue;
    assert!(queue.shuffle && queue.repeat);
    h.get_by_label("Shuffle").click();
    settle(&mut h);
    assert!(!h.state_mut().front().session.queue.shuffle);
}

#[test]
fn what_the_file_dialogs_return_is_acted_on() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    let sender = h.state_mut().front().picker.sender();

    sender
        .send(Picked::Media(vec![fx.path("single/film.mp4")]))
        .unwrap();
    settle(&mut h);
    let front = h.state_mut().front();
    assert_eq!(
        front.session.queue.current,
        Some(0),
        "picked media starts playing"
    );
    assert_eq!(
        front.session.store.settings.last_dir,
        fx.path("single"),
        "the picker reopens where you were"
    );

    let sub = fx.path("subs.srt");
    sender.send(Picked::Subtitle(sub.clone())).unwrap();
    let saved = fx.path("saved-queue.m3u");
    sender
        .send(Picked::SaveQueue(saved.clone().into()))
        .unwrap();
    settle(&mut h);
    let source = h.state_mut().front().session.source().unwrap();
    assert_eq!(
        h.state_mut()
            .front()
            .session
            .store
            .file_state(&source)
            .subtitles,
        [sub]
    );
    let text = std::fs::read_to_string(&saved).unwrap();
    assert!(
        text.starts_with("#EXTM3U") && text.contains("film.mp4"),
        "{text}"
    );
}

#[test]
fn the_play_button_and_space_both_pause() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("single/film.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing
        && s.position_ms() > 0));
    settle(&mut h);
    h.get_by_label("Now playing");
    first(&h, "film.mp4");

    h.get_by_label("Pause").click();
    settle(&mut h);
    assert_eq!(h.state_mut().front().session.state(), State::Paused);
    h.get_by_label("Play");

    h.key_press(Key::Space);
    settle(&mut h);
    assert!(wait(&mut h.state_mut().front().session, 3, |s| s.state()
        == State::Playing));
}

#[test]
fn keys_are_ignored_while_typing_in_a_field() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("single/film.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    settle(&mut h);
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .focus();
    settle(&mut h);
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .type_text(" ");
    settle(&mut h);
    assert_eq!(
        h.state_mut().front().session.state(),
        State::Playing,
        "a space typed in the filter must not pause"
    );
}

#[test]
fn a_shortcut_can_be_rebound_in_preferences_and_then_used() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("single/film.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));

    h.key_press_modifiers(Modifiers::CTRL, Key::Comma);
    settle(&mut h);
    first(&h, "Preferences");
    h.get_by_label("Keyboard").click();
    settle(&mut h);
    first(&h, "Mute");

    // Mute is bound to M; capture a new key for it.
    h.get_by_label("Shortcut for Mute").scroll_to_me();
    settle(&mut h);
    h.get_by_label("Shortcut for Mute").click();
    settle(&mut h);
    assert_eq!(
        h.state_mut().front().prefs.as_ref().and_then(|p| p.capture),
        Some(Action::Mute)
    );

    // A key another action uses is refused, and the row keeps waiting.
    h.key_press(Key::Space);
    settle(&mut h);
    h.get_by_label("Space is already used by “Play / pause”.");
    assert!(h.state_mut().front().session.store.settings.keys.is_empty());

    h.key_press_modifiers(Modifiers::CTRL, Key::U);
    settle(&mut h);
    assert_eq!(
        h.state_mut()
            .front()
            .session
            .store
            .settings
            .keys
            .get("mute")
            .map(String::as_str),
        Some("Ctrl+U")
    );
    assert_eq!(
        h.state_mut().front().prefs.as_ref().and_then(|p| p.capture),
        None
    );

    // The new key works, and the old one no longer does.
    h.key_press(Key::M);
    settle(&mut h);
    assert!(
        !h.state_mut().front().session.engine.muted(),
        "M is free now"
    );
    h.key_press_modifiers(Modifiers::CTRL, Key::U);
    settle(&mut h);
    assert!(h.state_mut().front().session.engine.muted(), "Ctrl+U mutes");
}

#[test]
fn escape_cancels_a_key_capture_without_closing_preferences() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .open_prefs(lumen_rs::ui::PrefsTab::Keys);
    settle(&mut h);
    h.get_by_label("Shortcut for Mute").scroll_to_me();
    settle(&mut h);
    h.get_by_label("Shortcut for Mute").click();
    settle(&mut h);
    h.key_press(Key::Escape);
    h.step();
    settle(&mut h);
    assert_eq!(
        h.state_mut().front().prefs.as_ref().and_then(|p| p.capture),
        None
    );
    first(&h, "Preferences");
    assert!(h.state_mut().front().session.store.settings.keys.is_empty());
}

#[test]
fn preferences_change_settings_immediately() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .open_prefs(lumen_rs::ui::PrefsTab::Playback);
    settle(&mut h);
    assert!(h.state_mut().front().session.store.settings.resume);
    h.get_by_label("Resume where you left off").click();
    settle(&mut h);
    assert!(!h.state_mut().front().session.store.settings.resume);

    h.get_by_label("Streams").click();
    settle(&mut h);
    h.get_by_label("Low-latency mode for Twitch and Kick")
        .click();
    settle(&mut h);
    assert!(!h.state_mut().front().session.store.settings.low_latency);
    // With SponsorBlock off the intros option has nothing to refine.
    assert!(
        h.get_by_label("Also skip intros and outros")
            .accesskit_node()
            .is_disabled()
            == !h.state_mut().front().session.store.settings.sponsorblock
    );
}

#[test]
fn the_more_menu_opens_preferences_and_media_information() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("single/film.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    settle(&mut h);

    h.get_by_label("…").click();
    settle(&mut h);
    h.get_by_label("Media information").click();
    settle(&mut h);
    first(&h, "film.mp4");
    pass_time(&mut h, 400);
    h.get_by_label("Resolution");
    h.get_by_label("320 × 180");

    h.get_by_label("…").click();
    settle(&mut h);
    h.get_by_label("Keyboard shortcuts").click();
    settle(&mut h);
    h.get_by_label("Click a shortcut, then press the new keys. Escape cancels.");
}

#[test]
fn the_audio_menu_switches_tracks() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("tracks.mkv")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    settle(&mut h);
    let before = h
        .state_mut()
        .front()
        .session
        .engine
        .selected_track(lumen_rs::engine::TrackKind::Audio);

    h.get_by_label("Audio & subtitles").click();
    settle(&mut h);
    h.get_by_label("Audio track ⏵").click();
    settle(&mut h);
    h.get_by_label("jpn").click();
    settle(&mut h);
    let after = h
        .state_mut()
        .front()
        .session
        .engine
        .selected_track(lumen_rs::engine::TrackKind::Audio);
    assert_ne!(before, after, "choosing the second track switches to it");
}

#[test]
fn the_video_menu_lists_chapters_and_jumps() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("chapters.mkv")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    pass_time(&mut h, 600);

    h.get_by_label("Video").click();
    settle(&mut h);
    h.get_by_label("Chapters ⏵").click();
    settle(&mut h);
    h.get_by_label("Intro   0:00");
    h.get_by_label("Main   0:10").click();
    settle(&mut h);
    assert!(wait(&mut h.state_mut().front().session, 4, |s| s
        .position_ms()
        >= 9_500));
}

#[test]
fn the_mini_player_opens_and_closes_with_its_shortcut() {
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("single/film.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    settle(&mut h);
    assert!(h.query_by_label("Now playing").is_some());

    h.key_press_modifiers(Modifiers::CTRL, Key::M);
    settle(&mut h);
    assert!(h.state_mut().front().is_mini());
    assert!(
        h.query_by_label("PLAY QUEUE  ·  1").is_none(),
        "the sidebar steps aside"
    );

    h.key_press_modifiers(Modifiers::CTRL, Key::M);
    settle(&mut h);
    assert!(!h.state_mut().front().is_mini());
    h.get_by_label("Now playing");
}

#[test]
fn the_queue_toggle_hides_and_shows_the_sidebar() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.get_by_label("PLAY QUEUE  ·  0");
    h.key_press(Key::Q);
    settle(&mut h);
    assert!(h.query_by_label("PLAY QUEUE  ·  0").is_none());
    assert!(!h.state_mut().front().session.store.settings.show_queue);
    h.key_press(Key::Q);
    settle(&mut h);
    h.get_by_label("PLAY QUEUE  ·  0");
}

#[test]
fn double_clicking_a_queue_row_plays_it() {
    let Some(fx) = fixture() else { return };
    let mut h = harness_timed(quiet_session(), [1100.0, 720.0], 0.05);
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("show/ep1.mp4"), fx.path("show/ep2.mp4")], false);
    settle(&mut h);
    assert_eq!(h.state_mut().front().session.queue.current, None);
    first(&h, "ep2.mp4").click();
    h.step();
    first(&h, "ep2.mp4").click();
    h.step();
    settle(&mut h);
    assert_eq!(h.state_mut().front().session.queue.current, Some(1));
}

#[test]
fn the_layout_survives_odd_window_sizes_and_states() {
    let Some(fx) = fixture() else { return };
    for size in [
        [1920.0, 1080.0],
        [900.0, 600.0],
        [620.0, 620.0],
        [420.0, 300.0],
    ] {
        let mut s = quiet_session();
        s.add(
            &[
                fx.path("tracks.mkv"),
                "https://example.test/a-very-long-title-".repeat(8),
            ],
            false,
        );
        let mut h = harness_sized(s, size);
        pass_time(&mut h, 100);
        h.get_by_label("Welcome");
        h.state_mut().front().session.play_index(0, None, false);
        pass_time(&mut h, 300);
        h.state_mut()
            .front()
            .open_prefs(lumen_rs::ui::PrefsTab::Keys);
        h.state_mut().front().info = true;
        pass_time(&mut h, 100);
    }
}

#[test]
fn the_welcome_content_steps_aside_while_files_hover_over_the_window() {
    let Some(_fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.get_by_label("Drop media here");
    h.input_mut().hovered_files = vec![egui::HoveredFile {
        path: Some("/tmp/a.mp4".into()),
        mime: String::new(),
    }];
    h.step();
    assert!(
        h.query_by_label("Drop media here").is_none(),
        "the drop hint must not be drawn over the welcome text"
    );
    h.input_mut().hovered_files.clear();
    h.run_steps(2);
    h.get_by_label("Drop media here");
}

/// The demo recorder (`examples/demo.rs`) aims at these widgets by name. Renaming one would
/// quietly break the recording, so this keeps the names honest.
#[test]
fn the_widgets_the_demo_recorder_aims_at_report_where_they_are() {
    use lumen_rs::ui::geometry::find;
    let Some(fx) = fixture() else { return };
    let mut h = harness(quiet_session());
    h.state_mut()
        .front()
        .session
        .add(&[fx.path("tracks.mkv"), fx.path("show/ep1.mp4")], true);
    assert!(wait(&mut h.state_mut().front().session, 8, |s| s.state()
        == State::Playing));
    pass_time(&mut h, 600);
    let ctx = h.ctx.clone();
    for name in [
        "Seek bar",
        "Volume bar",
        "Speed",
        "Video area",
        "Audio & subtitles",
        "Video",
        "…",
        "tracks.mkv",
        "ep1.mp4",
    ] {
        let rect =
            find(&ctx, name).unwrap_or_else(|| panic!("{name:?} did not report its position"));
        assert!(
            rect.width() > 0.0 && rect.height() > 0.0,
            "{name:?} has no size"
        );
    }

    // Menu entries report themselves only while open.
    h.get_by_label("Video").click();
    settle(&mut h);
    for name in ["Chapters", "Frame", "Take screenshot", "Fullscreen"] {
        assert!(
            find(&ctx, name).is_some(),
            "{name:?} did not report its position"
        );
    }
    h.key_press(Key::Escape);
    settle(&mut h);
    h.get_by_label("…").click();
    settle(&mut h);
    for name in ["Preferences…", "Media information", "Keyboard shortcuts"] {
        assert!(
            find(&ctx, name).is_some(),
            "{name:?} did not report its position"
        );
    }
}
