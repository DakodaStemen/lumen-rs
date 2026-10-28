//! Every action Lumen can bind to a key, with Lumen's defaults and user overrides.

use std::collections::BTreeMap;

use eframe::egui::{Key, Modifiers};

macro_rules! actions {
    ($($name:ident, $id:literal, $label:literal, $key:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Action { $($name),* }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$name),*];

            /// Stable name used in the settings file.
            pub fn id(self) -> &'static str { match self { $(Action::$name => $id),* } }
            pub fn label(self) -> &'static str { match self { $(Action::$name => $label),* } }
            pub fn default_key(self) -> &'static str { match self { $(Action::$name => $key),* } }
        }
    };
}

actions! {
    Play, "play", "Play / pause", "Space";
    Stop, "stop", "Stop", "S";
    Forward, "forward", "Seek forward", "Right";
    Back, "back", "Seek back", "Left";
    ForwardLong, "forward_long", "Seek forward 1 min", "Shift+Right";
    BackLong, "back_long", "Seek back 1 min", "Shift+Left";
    Restart, "restart", "Restart file", "Home";
    Next, "next", "Next in queue", "N";
    Previous, "previous", "Previous in queue", "P";
    Frame, "frame", "Next frame", ".";
    FrameBack, "frame_back", "Previous frame", ",";
    ChapterNext, "chapter_next", "Next chapter", "PgDown";
    ChapterPrev, "chapter_prev", "Previous chapter", "PgUp";
    Faster, "faster", "Faster", "]";
    Slower, "slower", "Slower", "[";
    Normal, "normal", "Normal speed", "=";
    VolumeUp, "volume_up", "Volume up", "Up";
    VolumeDown, "volume_down", "Volume down", "Down";
    Mute, "mute", "Mute", "M";
    AudioDelayUp, "audio_delay_up", "Audio delay +50 ms", "K";
    AudioDelayDown, "audio_delay_down", "Audio delay -50 ms", "J";
    SubDelayUp, "sub_delay_up", "Subtitle delay +50 ms", "H";
    SubDelayDown, "sub_delay_down", "Subtitle delay -50 ms", "G";
    CycleAudio, "cycle_audio", "Cycle audio track", "B";
    CycleSub, "cycle_sub", "Cycle subtitle track", "V";
    Screenshot, "screenshot", "Screenshot", "Ctrl+Shift+S";
    Loop, "loop", "A-B loop (set A, set B, clear)", "L";
    Bookmark, "bookmark", "Add bookmark", "Ctrl+B";
    Fullscreen, "fullscreen", "Fullscreen", "F";
    ExitFullscreen, "exit_fullscreen", "Leave fullscreen", "Escape";
    Open, "open", "Open media", "Ctrl+O";
    OpenUrl, "network", "Open network stream", "Ctrl+L";
    Mini, "mini", "Mini player", "Ctrl+M";
    OnTop, "on_top", "Always on top", "Ctrl+T";
    Preferences, "preferences", "Preferences", "Ctrl+,";
    ToggleQueue, "toggle_queue", "Show / hide play queue", "Q";
    Remove, "remove", "Remove queue item", "Delete";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Binding {
    /// Parses text such as `Space`, `Shift+Right` or `Ctrl+Shift+S`.
    pub fn parse(text: &str) -> Option<Binding> {
        let (mods, key) = match text.trim().rsplit_once('+') {
            Some((mods, key)) if !key.is_empty() => (mods, key),
            // A lone "+" is the key itself.
            _ => ("", text.trim()),
        };
        let mut modifiers = Modifiers::NONE;
        for word in mods.split('+').filter(|w| !w.is_empty()) {
            match word.trim().to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" => modifiers.alt = true,
                "meta" | "cmd" | "super" => modifiers.mac_cmd = true,
                _ => return None,
            }
        }
        Some(Binding {
            key: parse_key(key)?,
            modifiers,
        })
    }

    /// The text form of a pressed key, which `parse` reads back. `None` for keys Lumen cannot bind.
    pub fn format(key: Key, modifiers: Modifiers) -> Option<String> {
        let name = match key {
            Key::Space => "Space",
            Key::ArrowRight => "Right",
            Key::ArrowLeft => "Left",
            Key::ArrowUp => "Up",
            Key::ArrowDown => "Down",
            Key::PageDown => "PgDown",
            Key::PageUp => "PgUp",
            Key::Escape => "Escape",
            Key::Delete => "Delete",
            Key::Enter => "Enter",
            Key::Period => ".",
            Key::Comma => ",",
            Key::OpenBracket => "[",
            Key::CloseBracket => "]",
            Key::Equals => "=",
            Key::Minus => "-",
            Key::Plus => "+",
            Key::Slash => "/",
            Key::Semicolon => ";",
            Key::Quote => "'",
            Key::Backtick => "`",
            Key::Backslash => "\\",
            other => other.name(),
        };
        let mut text = String::new();
        for (held, word) in [
            (modifiers.ctrl, "Ctrl"),
            (modifiers.alt, "Alt"),
            (modifiers.shift, "Shift"),
        ] {
            if held {
                text.push_str(word);
                text.push('+');
            }
        }
        text.push_str(name);
        (Self::parse(&text)?.key == key).then_some(text)
    }

    fn weight(&self) -> u32 {
        [
            self.modifiers.ctrl,
            self.modifiers.shift,
            self.modifiers.alt,
            self.modifiers.mac_cmd,
        ]
        .into_iter()
        .filter(|m| *m)
        .count() as u32
    }
}

fn parse_key(text: &str) -> Option<Key> {
    Some(match text.to_lowercase().as_str() {
        "space" => Key::Space,
        "right" => Key::ArrowRight,
        "left" => Key::ArrowLeft,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "pgdown" | "pagedown" => Key::PageDown,
        "pgup" | "pageup" => Key::PageUp,
        "esc" | "escape" => Key::Escape,
        "del" | "delete" => Key::Delete,
        "return" | "enter" => Key::Enter,
        "." | "period" => Key::Period,
        "," | "comma" => Key::Comma,
        "[" => Key::OpenBracket,
        "]" => Key::CloseBracket,
        "=" => Key::Equals,
        "-" => Key::Minus,
        "+" => Key::Plus,
        "/" => Key::Slash,
        ";" => Key::Semicolon,
        "'" => Key::Quote,
        "`" => Key::Backtick,
        "\\" => Key::Backslash,
        other => Key::from_name(&other.to_uppercase()).or_else(|| Key::from_name(text))?,
    })
}

/// The effective bindings: Lumen's defaults with the user's overrides applied.
pub struct Keymap {
    bindings: Vec<(Action, Binding)>,
}

impl Keymap {
    pub fn from_settings(overrides: &BTreeMap<String, String>) -> Keymap {
        let mut bindings: Vec<_> = Action::ALL
            .iter()
            .filter_map(|&action| {
                Binding::parse(&binding_text(overrides, action)).map(|b| (action, b))
            })
            .collect();
        // Check `Shift+Right` before `Right` so the more specific shortcut wins.
        bindings.sort_by_key(|(_, b)| std::cmp::Reverse(b.weight()));
        Keymap { bindings }
    }

    pub fn bindings(&self) -> &[(Action, Binding)] {
        &self.bindings
    }
}

/// Binds `action` to `text`. Fails with the action that already uses that key, changing nothing.
pub fn rebind(
    overrides: &mut BTreeMap<String, String>,
    action: Action,
    text: &str,
) -> Result<(), Action> {
    let wanted = Binding::parse(text);
    if let Some(wanted) = wanted
        && let Some(other) = Action::ALL
            .iter()
            .copied()
            .find(|&a| a != action && Binding::parse(&binding_text(overrides, a)) == Some(wanted))
    {
        return Err(other);
    }
    if text == action.default_key() {
        overrides.remove(action.id());
    } else {
        overrides.insert(action.id().to_owned(), text.to_owned());
    }
    Ok(())
}

/// Leaves `action` with no key.
pub fn unbind(overrides: &mut BTreeMap<String, String>, action: Action) {
    overrides.insert(action.id().to_owned(), String::new());
}

/// Puts `action` back on its default key, unless another action has taken it.
pub fn reset(overrides: &mut BTreeMap<String, String>, action: Action) -> Result<(), Action> {
    rebind(overrides, action, action.default_key())
}

/// The key text for `action`, as the user would type it.
pub fn binding_text(overrides: &BTreeMap<String, String>, action: Action) -> String {
    overrides
        .get(action.id())
        .cloned()
        .unwrap_or_else(|| action.default_key().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_parses() {
        for &action in Action::ALL {
            assert!(
                Binding::parse(action.default_key()).is_some(),
                "{} = {}",
                action.id(),
                action.default_key()
            );
        }
    }

    #[test]
    fn action_ids_are_unique() {
        let mut ids: Vec<_> = Action::ALL.iter().map(|a| a.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Action::ALL.len());
    }

    #[test]
    fn default_keys_do_not_collide() {
        let map = Keymap::from_settings(&BTreeMap::new());
        let mut seen = Vec::new();
        for (action, binding) in map.bindings() {
            assert!(!seen.contains(binding), "{} reuses a key", action.id());
            seen.push(*binding);
        }
    }

    #[test]
    fn parses_modifiers_and_symbol_keys() {
        let b = Binding::parse("Ctrl+Shift+S").unwrap();
        assert_eq!(b.key, Key::S);
        assert!(b.modifiers.ctrl && b.modifiers.shift && !b.modifiers.alt);
        assert_eq!(Binding::parse("]").unwrap().key, Key::CloseBracket);
        assert_eq!(Binding::parse("Shift+Right").unwrap().key, Key::ArrowRight);
        assert_eq!(Binding::parse("PgDown").unwrap().key, Key::PageDown);
        assert!(Binding::parse("Hyper+X").is_none());
        assert!(Binding::parse("").is_none());
    }

    #[test]
    fn overrides_rebind_and_unbind() {
        let mut overrides = BTreeMap::new();
        overrides.insert("mute".to_owned(), "Ctrl+U".to_owned());
        overrides.insert("stop".to_owned(), String::new());
        let map = Keymap::from_settings(&overrides);
        let find = |a| {
            map.bindings()
                .iter()
                .find(|(x, _)| *x == a)
                .map(|(_, b)| *b)
        };
        assert_eq!(find(Action::Mute), Binding::parse("Ctrl+U"));
        assert_eq!(find(Action::Stop), None);
        assert!(find(Action::Play).is_some());
    }

    #[test]
    fn every_bindable_key_reads_back_as_itself() {
        let mut bindable = 0;
        for &key in Key::ALL {
            let Some(text) = Binding::format(key, Modifiers::NONE) else {
                continue;
            };
            assert_eq!(Binding::parse(&text).unwrap().key, key, "{text}");
            bindable += 1;
        }
        assert!(
            bindable > 60,
            "letters, digits, arrows, F-keys and symbols are bindable ({bindable})"
        );
        let both = Binding::format(
            Key::S,
            Modifiers {
                ctrl: true,
                shift: true,
                ..Modifiers::NONE
            },
        )
        .unwrap();
        assert_eq!(both, "Ctrl+Shift+S");
        assert_eq!(Binding::parse(&both), Binding::parse("Ctrl+Shift+S"));
    }

    #[test]
    fn rebinding_refuses_a_key_that_is_taken() {
        let mut keys = BTreeMap::new();
        assert_eq!(rebind(&mut keys, Action::Mute, "Space"), Err(Action::Play));
        assert!(keys.is_empty(), "a refused rebind changes nothing");
        assert_eq!(rebind(&mut keys, Action::Mute, "Ctrl+U"), Ok(()));
        assert_eq!(rebind(&mut keys, Action::Stop, "Ctrl+U"), Err(Action::Mute));
        // The key Mute gave up is free again.
        assert_eq!(rebind(&mut keys, Action::Stop, "M"), Ok(()));
    }

    #[test]
    fn rebinding_to_the_default_removes_the_override() {
        let mut keys = BTreeMap::new();
        rebind(&mut keys, Action::Mute, "Ctrl+U").unwrap();
        assert_eq!(reset(&mut keys, Action::Mute), Ok(()));
        assert!(keys.is_empty());
    }

    #[test]
    fn a_reset_is_refused_while_another_action_holds_the_default() {
        let mut keys = BTreeMap::new();
        rebind(&mut keys, Action::Mute, "Ctrl+U").unwrap();
        rebind(&mut keys, Action::Stop, "M").unwrap();
        assert_eq!(reset(&mut keys, Action::Mute), Err(Action::Stop));
        assert_eq!(binding_text(&keys, Action::Mute), "Ctrl+U");
    }

    #[test]
    fn unbinding_leaves_no_key() {
        let mut keys = BTreeMap::new();
        unbind(&mut keys, Action::Stop);
        assert!(
            Keymap::from_settings(&keys)
                .bindings()
                .iter()
                .all(|(a, _)| *a != Action::Stop)
        );
        assert_eq!(
            rebind(&mut keys, Action::Mute, "S"),
            Ok(()),
            "an unbound action's key can be reused"
        );
    }

    #[test]
    fn specific_shortcuts_are_checked_first() {
        let map = Keymap::from_settings(&BTreeMap::new());
        let at = |a| map.bindings().iter().position(|(x, _)| *x == a).unwrap();
        assert!(at(Action::ForwardLong) < at(Action::Forward));
    }
}
