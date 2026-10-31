//! Turns Lumen's settings into mpv options. Pure functions, so the mapping is tested without mpv.

use std::path::{Path, PathBuf};

use crate::store::Settings;

/// Hardware decoding choices offered in Preferences: `(mpv value, label)`.
pub const HWDEC: [(&str, &str); 4] = [
    ("auto-safe", "Automatic (safe)"),
    ("vaapi", "VA-API"),
    ("nvdec", "NVDEC"),
    ("no", "Off (software)"),
];

/// Tone-mapping algorithms that mpv's `gpu` renderer accepts.
pub const TONE_MAPPING: [&str; 12] = [
    "auto",
    "clip",
    "mobius",
    "reinhard",
    "hable",
    "gamma",
    "linear",
    "spline",
    "bt.2390",
    "bt.2446a",
    "st2094-40",
    "st2094-10",
];

pub const REPLAYGAIN: [(&str, &str); 3] = [("no", "Off"), ("track", "Track"), ("album", "Album")];

/// Aspect-ratio overrides: `(mpv value, label)`. `-1` restores the file's own ratio.
pub const ASPECTS: [(&str, &str); 6] = [
    ("-1", "Original"),
    ("16:9", "16:9"),
    ("4:3", "4:3"),
    ("21:9", "21:9"),
    ("1:1", "1:1"),
    ("2.35:1", "2.35:1"),
];

/// Crop presets: a centred area with this ratio, or `None` for no crop.
pub const CROPS: [(Option<(u32, u32)>, &str); 4] = [
    (None, "No crop"),
    (Some((16, 9)), "16:9"),
    (Some((21, 9)), "21:9"),
    (Some((4, 3)), "4:3"),
];

/// The audio filter chain for night mode: EBU R128 loudness with a narrow loudness range,
/// so quiet dialogue comes up and explosions come down.
pub const NIGHT_MODE_FILTER: &str = "lavfi=[loudnorm=I=-18:TP=-2:LRA=7]";

pub fn valid_hwdec(value: &str) -> bool {
    HWDEC.iter().any(|(v, _)| *v == value)
}

pub fn valid_tone_mapping(value: &str) -> bool {
    TONE_MAPPING.contains(&value)
}

/// `#RRGGBB` (the `#` is optional) as the uppercase form mpv takes, or `None` when malformed.
pub fn normalize_color(text: &str) -> Option<String> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_uppercase()))
}

/// `#RRGGBB` as an RGB triple.
pub fn color_rgb(text: &str) -> Option<[u8; 3]> {
    let hex = normalize_color(text)?;
    let n = u32::from_str_radix(&hex[1..], 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// The folder user shader files are read from. Lumen bundles none.
pub fn shader_dir() -> PathBuf {
    crate::store::Store::default_dir().join("shaders")
}

/// Shader files in `dir`, sorted by name. Only plain files with a shader extension count.
pub fn list_shaders(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "glsl" | "hook" | "frag"))
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// The `glsl-shaders` value for the chosen shader names: absolute paths of the ones that exist
/// in `dir`, in the order chosen. Names with path separators are refused.
pub fn shader_list(dir: &Path, chosen: &[String]) -> String {
    chosen
        .iter()
        .filter(|n| !n.contains(['/', '\\', ':']) && *n != ".." && *n != ".")
        .map(|n| dir.join(n))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(":")
}

/// Every mpv option Lumen's settings control, as `(name, value)` pairs.
/// Invalid stored values fall back to defaults instead of reaching mpv.
pub fn mpv_options(settings: &Settings, shader_dir: &Path) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    out.push((
        "hwdec",
        if valid_hwdec(&settings.hwdec) {
            settings.hwdec.clone()
        } else {
            "auto-safe".to_owned()
        },
    ));
    out.push((
        "tone-mapping",
        if valid_tone_mapping(&settings.tone_mapping) {
            settings.tone_mapping.clone()
        } else {
            "auto".to_owned()
        },
    ));

    let sub = &settings.subtitles;
    out.push(("sub-font", sub.font.trim().to_owned()));
    out.push(("sub-font-size", sub.size.clamp(8, 200).to_string()));
    out.push((
        "sub-color",
        normalize_color(&sub.color).unwrap_or_else(|| "#FFFFFF".to_owned()),
    ));
    out.push((
        "sub-border-color",
        normalize_color(&sub.border_color).unwrap_or_else(|| "#000000".to_owned()),
    ));
    out.push(("sub-border-size", format!("{:.1}", sub.border.clamp(0.0, 10.0))));
    out.push(("sub-pos", sub.pos.min(100).to_string()));

    let pic = &settings.picture;
    for (name, value) in [
        ("brightness", pic.brightness),
        ("contrast", pic.contrast),
        ("saturation", pic.saturation),
        ("gamma", pic.gamma),
        ("hue", pic.hue),
    ] {
        out.push((name, value.clamp(-100, 100).to_string()));
    }

    let audio = &settings.audio;
    out.push((
        "af",
        if audio.night_mode {
            NIGHT_MODE_FILTER.to_owned()
        } else {
            String::new()
        },
    ));
    out.push((
        "replaygain",
        REPLAYGAIN
            .iter()
            .find(|(v, _)| *v == audio.replaygain)
            .map_or("no", |(v, _)| *v)
            .to_owned(),
    ));
    out.push((
        "audio-device",
        if audio.device.trim().is_empty() {
            "auto".to_owned()
        } else {
            audio.device.clone()
        },
    ));
    out.push(("glsl-shaders", shader_list(shader_dir, &settings.shaders)));
    out
}

/// A centred crop of a `width`×`height` picture with the ratio `ratio`, as mpv's
/// `video-crop` value (`WxH+X+Y`). `None` when the picture already is narrower than the ratio.
pub fn crop_geometry(width: i64, height: i64, ratio: (u32, u32)) -> Option<String> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let (rw, rh) = (i64::from(ratio.0), i64::from(ratio.1));
    // Largest centred rectangle with the wanted ratio, even-sized for chroma subsampling.
    let (w, h) = if width * rh >= height * rw {
        (height * rw / rh / 2 * 2, height)
    } else {
        (width, width * rh / rw / 2 * 2)
    };
    Some(format!("{w}x{h}+{}+{}", (width - w) / 2, (height - h) / 2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option<'a>(opts: &'a [(&'static str, String)], name: &str) -> &'a str {
        &opts.iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn defaults_map_to_mpv_defaults() {
        let opts = mpv_options(&Settings::default(), Path::new("/nonexistent"));
        assert_eq!(option(&opts, "hwdec"), "auto-safe");
        assert_eq!(option(&opts, "sub-font-size"), "55");
        assert_eq!(option(&opts, "af"), "");
        assert_eq!(option(&opts, "replaygain"), "no");
        assert_eq!(option(&opts, "glsl-shaders"), "");
    }

    #[test]
    fn invalid_stored_values_do_not_reach_mpv() {
        let mut s = Settings::default();
        s.hwdec = "rm -rf".into();
        s.tone_mapping = "bogus".into();
        s.subtitles.color = "red".into();
        s.subtitles.size = 9999;
        s.picture.gamma = 500;
        s.audio.replaygain = "x".into();
        let opts = mpv_options(&s, Path::new("/nonexistent"));
        assert_eq!(option(&opts, "hwdec"), "auto-safe");
        assert_eq!(option(&opts, "tone-mapping"), "auto");
        assert_eq!(option(&opts, "sub-color"), "#FFFFFF");
        assert_eq!(option(&opts, "sub-font-size"), "200");
        assert_eq!(option(&opts, "gamma"), "100");
        assert_eq!(option(&opts, "replaygain"), "no");
    }

    #[test]
    fn night_mode_sets_the_loudnorm_chain() {
        let mut s = Settings::default();
        s.audio.night_mode = true;
        let opts = mpv_options(&s, Path::new("/x"));
        assert!(option(&opts, "af").contains("loudnorm"));
    }

    #[test]
    fn colours_normalise() {
        assert_eq!(normalize_color(" ffcc00 ").as_deref(), Some("#FFCC00"));
        assert_eq!(normalize_color("#12345"), None);
        assert_eq!(color_rgb("#FF8000"), Some([255, 128, 0]));
    }

    #[test]
    fn shaders_are_listed_and_confined_to_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.glsl", "A.hook", "notes.txt", "c.frag"] {
            std::fs::write(dir.path().join(name), "//").unwrap();
        }
        std::fs::create_dir(dir.path().join("sub.glsl")).unwrap();
        assert_eq!(list_shaders(dir.path()), ["A.hook", "b.glsl", "c.frag"]);
        let chosen = ["b.glsl".to_owned(), "../evil.glsl".to_owned(), "gone.glsl".to_owned()];
        let list = shader_list(dir.path(), &chosen);
        assert_eq!(list, dir.path().join("b.glsl").to_string_lossy());
        assert!(list_shaders(Path::new("/does/not/exist")).is_empty());
    }

    #[test]
    fn crop_is_centred_and_even() {
        assert_eq!(crop_geometry(1920, 1080, (21, 9)).as_deref(), Some("1920x822+0+129"));
        assert_eq!(crop_geometry(1920, 1080, (4, 3)).as_deref(), Some("1440x1080+240+0"));
        assert_eq!(crop_geometry(0, 0, (4, 3)), None);
    }
}
