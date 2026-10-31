//! Quality, subtitle, picture and audio options reach real (headless) mpv.

mod common;

use common::{fixture, play, session, wait};
use lumen_rs::session::State;

#[test]
fn settings_reach_mpv() {
    let mut s = session();
    {
        let c = &mut s.store.settings;
        c.hwdec = "no".into();
        c.tone_mapping = "hable".into();
        c.subtitles.size = 40;
        c.subtitles.color = "#ffcc00".into();
        c.subtitles.border = 2.0;
        c.subtitles.pos = 90;
        c.picture.brightness = 12;
        c.picture.contrast = -7;
        c.picture.saturation = 5;
        c.picture.gamma = 3;
        c.picture.hue = -4;
        c.audio.night_mode = true;
        c.audio.replaygain = "album".into();
    }
    s.apply_settings();
    assert_eq!(s.flash_message(), None, "mpv refused an option: {:?}", s.flash_message());
    let e = &s.engine;
    assert_eq!(e.property("hwdec").as_deref(), Some("no"));
    assert_eq!(e.property("tone-mapping").as_deref(), Some("hable"));
    assert_eq!(e.property("sub-font-size").as_deref(), Some("40"));
    assert_eq!(e.property("sub-pos").as_deref(), Some("90"));
    assert_eq!(e.property("brightness").as_deref(), Some("12"));
    assert_eq!(e.property("contrast").as_deref(), Some("-7"));
    assert_eq!(e.property("hue").as_deref(), Some("-4"));
    assert_eq!(e.property("replaygain").as_deref(), Some("album"));
    assert!(e.property("af").is_some_and(|f| f.contains("loudnorm")), "{:?}", e.property("af"));
    assert!(e.property("sub-color").is_some_and(|c| c.to_uppercase().contains("FFCC00")), "{:?}", e.property("sub-color"));

    s.store.settings.audio.night_mode = false;
    s.apply_settings();
    assert!(!e_af_has_loudnorm(&s));
}

fn e_af_has_loudnorm(s: &lumen_rs::session::Session) -> bool {
    s.engine.property("af").is_some_and(|f| f.contains("loudnorm"))
}

#[test]
fn audio_devices_include_auto() {
    let s = session();
    let devices = s.engine.audio_devices();
    assert!(devices.iter().any(|(name, _)| name == "auto"), "{devices:?}");
}

#[test]
fn aspect_rotation_and_crop_apply_to_the_playing_video() {
    let Some(fx) = fixture() else { return };
    let mut s = session();
    play(&mut s, &fx.path("single/film.mp4"));
    assert!(wait(&mut s, 5, |s| s.state() == State::Playing));
    s.engine.set_aspect("4:3");
    assert!(s.engine.aspect().starts_with("1.33"), "{}", s.engine.aspect());
    s.engine.set_aspect("-1");
    s.engine.set_rotation(90);
    assert_eq!(s.engine.rotation(), 90);
    s.engine.set_rotation(-90);
    assert_eq!(s.engine.rotation(), 270);
    assert!(s.engine.set_crop(Some((1, 1))), "crop refused");
    assert_eq!(s.engine.crop(), "180x180+70+0");
    assert!(s.engine.set_crop(None));
    assert_eq!(s.engine.crop(), "");
}
