//! Key events: the key the terminal reported, with everything it reported
//! about it, and opt-in views of the key that was meant.

use bitflags::bitflags;

/// A key press, repeat or release.
///
/// `key` is the key as the terminal reported it, unshifted where the encoding
/// says which key it was: under the kitty protocol Shift+a arrives as `a` with
/// [`KeyModifiers::SHIFT`], and the shifted character, where the terminal
/// reported it, is in `shifted`. Nothing is folded on the caller's behalf; use
/// [`KeyEvent::logical_char`] for the character that was meant.
///
/// Built with [`KeyEvent::new`] and the `with_*` methods, so a later field is
/// not a breaking change. `KeyEvent { key, modifiers, .. }` still matches.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct KeyEvent {
    /// The key: kitty's unshifted key code, or what a legacy or xterm
    /// sequence names.
    pub key: KeyCode,
    /// Shift, Alt, Ctrl, Super, Hyper and Meta. Lock keys are in `state`.
    pub modifiers: KeyModifiers,
    /// Press, repeat or release.
    pub kind: KeyEventKind,
    /// Keypad, Caps Lock and Num Lock.
    pub state: KeyEventState,
    /// The shifted key, where the terminal reported it (kitty flag 4) or the
    /// sequence carried it (an upper-case letter in legacy or xterm form).
    pub shifted: Option<char>,
    /// The key at this position in the standard PC-101 layout, where the
    /// terminal reported it (kitty flag 4).
    pub base_layout: Option<KeyCode>,
    /// The text the key produced, where the terminal reported it (kitty
    /// flag 16).
    pub text: Option<String>,
    /// The form of sequence the event came in.
    pub encoding: KeyEncoding,
}

impl KeyEvent {
    /// A press of `key` with `modifiers`, as an unambiguous encoding reports it
    /// ([`KeyEncoding::Kitty`]): a hand-built event is taken at face value.
    pub const fn new(key: KeyCode, modifiers: KeyModifiers) -> Self {
        Self {
            key,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
            shifted: None,
            base_layout: None,
            text: None,
            encoding: KeyEncoding::Kitty,
        }
    }

    /// This event with its kind set.
    #[must_use]
    pub const fn with_kind(mut self, kind: KeyEventKind) -> Self {
        self.kind = kind;
        self
    }

    /// This event with its state set.
    #[must_use]
    pub const fn with_state(mut self, state: KeyEventState) -> Self {
        self.state = state;
        self
    }

    /// This event with its shifted key set.
    #[must_use]
    pub const fn with_shifted(mut self, shifted: Option<char>) -> Self {
        self.shifted = shifted;
        self
    }

    /// This event with its base-layout key set.
    #[must_use]
    pub const fn with_base_layout(mut self, base_layout: Option<KeyCode>) -> Self {
        self.base_layout = base_layout;
        self
    }

    /// This event with its associated text set.
    #[must_use]
    pub fn with_text(mut self, text: Option<String>) -> Self {
        self.text = text;
        self
    }

    /// This event with its encoding set.
    #[must_use]
    pub const fn with_encoding(mut self, encoding: KeyEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// The character the key was meant to type, or `None` for a key that is
    /// not a character.
    ///
    /// In order: the associated text, where the terminal sent exactly one
    /// character of it; the shifted key, where Shift was held and the terminal
    /// reported it; else the key itself, upper-cased where Shift was held (any
    /// letter with a single upper-case form, not only ASCII). Caps Lock flips
    /// a letter's case only when no Ctrl, Alt, Super, Hyper or Meta is held:
    /// Ctrl+a with Caps Lock on is still Ctrl+a.
    ///
    /// [`KeyEvent::logical_modifiers`] says whether Shift is used up by the
    /// character returned.
    pub fn logical_char(&self) -> Option<char> {
        self.logical().map(|(c, _)| c)
    }

    /// The modifiers that go with [`KeyEvent::logical_char`]: Shift is cleared
    /// where the character already carries it (Shift+a as `A`), and kept where
    /// it is the only record that Shift was held (Alt+Shift+1 from a terminal
    /// that reported only `1`). For a key that is not a character, the
    /// modifiers as reported.
    pub fn logical_modifiers(&self) -> KeyModifiers {
        self.logical().map_or(self.modifiers, |(_, mods)| mods)
    }

    fn logical(&self) -> Option<(char, KeyModifiers)> {
        let shift = self.modifiers.contains(KeyModifiers::SHIFT);
        let without_shift = self.modifiers - KeyModifiers::SHIFT;
        if let Some(c) = self.text.as_deref().and_then(single_char) {
            let carries_shift = shift && Some(c) != self.key_char();
            return Some((
                c,
                if carries_shift {
                    without_shift
                } else {
                    self.modifiers
                },
            ));
        }
        let c = self.key_char()?;
        if shift && let Some(shifted) = self.shifted {
            return Some((shifted, without_shift));
        }
        let Some(upper) = single_upper(c) else {
            return Some((c, self.modifiers));
        };
        let chord = self.modifiers.intersects(
            KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::HYPER
                | KeyModifiers::META,
        );
        let caps = !chord && self.state.contains(KeyEventState::CAPS_LOCK);
        let c = if shift != caps { upper } else { c };
        Some((c, without_shift))
    }

    const fn key_char(&self) -> Option<char> {
        match self.key {
            KeyCode::Char(c) => Some(c),
            _ => None,
        }
    }

    /// This event with Ctrl+\[, Ctrl+I and Ctrl+M mapped to Esc, Tab and
    /// Enter, the keys a legacy terminal cannot tell them apart from, and,
    /// from a legacy terminal, Ctrl+H to Backspace.
    ///
    /// A mapping applies only when Ctrl is the only modifier held (Ctrl+Alt+\[
    /// is not Esc), and only where the encoding made the keys distinct: under
    /// the kitty protocol Ctrl+\[ is its own key, while a legacy terminal
    /// already sends it as Esc and sends Backspace as Ctrl+H on some setups.
    /// The kind and state are kept, so a mapped release stays a release.
    ///
    /// For key handling that wants vim's aliases; an editor that binds
    /// Ctrl+\[ apart from Esc uses the raw event.
    #[must_use]
    pub fn normalize_legacy(self) -> Self {
        if self.modifiers != KeyModifiers::CONTROL {
            return self;
        }
        let legacy = self.encoding == KeyEncoding::Legacy;
        let key = match self.key {
            KeyCode::Char('[') if !legacy => KeyCode::Esc,
            KeyCode::Char('i' | 'I') if !legacy => KeyCode::Tab,
            KeyCode::Char('m' | 'M') if !legacy => KeyCode::Enter,
            KeyCode::Char('h') if legacy => KeyCode::Backspace,
            _ => return self,
        };
        Self {
            key,
            modifiers: KeyModifiers::NONE,
            shifted: None,
            base_layout: None,
            text: None,
            ..self
        }
    }
}

/// `s`'s only character, if it has exactly one.
fn single_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

/// `c`'s upper case, where it is one character and differs from `c`.
fn single_upper(c: char) -> Option<char> {
    let mut upper = c.to_uppercase();
    let u = upper.next()?;
    (upper.next().is_none() && u != c).then_some(u)
}

/// A key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyCode {
    /// Backspace.
    Backspace,
    /// Enter.
    Enter,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Tab.
    Tab,
    /// Shift+Tab, as legacy terminals send it (`CSI Z`) and as the kitty
    /// protocol's Tab with Shift is decoded, Shift kept in the modifiers.
    BackTab,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// A function key: `F(1)` is F1.
    F(u8),
    /// A character key.
    Char(char),
    /// No key: kitty's key code 0, for text that came without a key.
    Null,
    /// Escape.
    Esc,
    /// Caps Lock (kitty flag 8).
    CapsLock,
    /// Scroll Lock (kitty flag 8).
    ScrollLock,
    /// Num Lock (kitty flag 8).
    NumLock,
    /// Print Screen (kitty flag 8).
    PrintScreen,
    /// Pause (kitty flag 8).
    Pause,
    /// Menu (kitty flag 8).
    Menu,
    /// The keypad's centre key, Begin.
    KeypadBegin,
    /// A media key (kitty flag 8).
    Media(MediaKeyCode),
    /// A modifier key itself (kitty flag 8). Its press does not add to
    /// `modifiers`: they are as the terminal reported them.
    Modifier(ModifierKeyCode),
}

/// A media key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MediaKeyCode {
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Play/Pause.
    PlayPause,
    /// Reverse.
    Reverse,
    /// Stop.
    Stop,
    /// Fast-forward.
    FastForward,
    /// Rewind.
    Rewind,
    /// Next track.
    TrackNext,
    /// Previous track.
    TrackPrevious,
    /// Record.
    Record,
    /// Volume down.
    LowerVolume,
    /// Volume up.
    RaiseVolume,
    /// Mute.
    MuteVolume,
}

/// A modifier key.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum ModifierKeyCode {
    /// Left Shift.
    LeftShift,
    /// Left Control.
    LeftControl,
    /// Left Alt.
    LeftAlt,
    /// Left Super.
    LeftSuper,
    /// Left Hyper.
    LeftHyper,
    /// Left Meta.
    LeftMeta,
    /// Right Shift.
    RightShift,
    /// Right Control.
    RightControl,
    /// Right Alt.
    RightAlt,
    /// Right Super.
    RightSuper,
    /// Right Hyper.
    RightHyper,
    /// Right Meta.
    RightMeta,
    /// ISO Level 3 Shift (AltGr).
    IsoLevel3Shift,
    /// ISO Level 5 Shift.
    IsoLevel5Shift,
}

bitflags! {
    /// The modifiers held with a key or mouse event.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct KeyModifiers: u8 {
        /// Shift.
        const SHIFT = 0b0000_0001;
        /// Control.
        const CONTROL = 0b0000_0010;
        /// Alt (Option on macOS).
        const ALT = 0b0000_0100;
        /// Super (the Windows or Command key), kitty protocol only.
        const SUPER = 0b0000_1000;
        /// Hyper, kitty protocol only.
        const HYPER = 0b0001_0000;
        /// Meta: kitty's Meta, or xterm's in `CSI 27 ; mods ; key ~`.
        const META = 0b0010_0000;
        /// No modifiers.
        const NONE = 0b0000_0000;
    }
}

bitflags! {
    /// Where a key is and which lock keys were on.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
    pub struct KeyEventState: u8 {
        /// The key is on the keypad.
        const KEYPAD = 0b0000_0001;
        /// Caps Lock was on (kitty protocol only).
        const CAPS_LOCK = 0b0000_0010;
        /// Num Lock was on (kitty protocol only).
        const NUM_LOCK = 0b0000_0100;
        /// None of these.
        const NONE = 0b0000_0000;
    }
}

/// Whether a key went down, repeated or came up.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyEventKind {
    /// Pressed. Every key from a terminal that does not report kinds.
    Press,
    /// Held down and repeating (kitty flag 2).
    Repeat,
    /// Released (kitty flag 2).
    Release,
}

/// The form of sequence a key event came in, which bounds what the terminal
/// could have reported.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, PartialOrd, PartialEq, Eq, Clone, Copy, Hash)]
pub enum KeyEncoding {
    /// Bytes, C0 controls, `ESC` + key as Alt, and the SS3 and CSI forms of
    /// the cursor, editing and function keys. Ctrl+\[ and Esc, Ctrl+I and Tab,
    /// Ctrl+M and Enter are the same bytes here.
    Legacy,
    /// xterm's modifyOtherKeys, `CSI 27 ; mods ; key ~`.
    Xterm,
    /// The kitty keyboard protocol's `CSI … u`, which xterm's
    /// `formatOtherKeys` form shares.
    Kitty,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // ── normalize_legacy: hjkl_kitty's cases ────────────────────────────────

    #[test]
    fn normalize_maps_ctrl_aliases() {
        for (c, key) in [
            ('[', KeyCode::Esc),
            ('i', KeyCode::Tab),
            ('I', KeyCode::Tab),
            ('m', KeyCode::Enter),
            ('M', KeyCode::Enter),
        ] {
            let out = ctrl(c).normalize_legacy();
            assert_eq!(out.key, key, "{c}");
            assert_eq!(out.modifiers, KeyModifiers::NONE, "{c}");
        }
    }

    #[test]
    fn normalize_passes_other_keys_through() {
        for event in [
            KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE),
            ctrl('a'),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            KeyEvent::new(
                KeyCode::Char('['),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            // Under kitty, Ctrl+H is its own key.
            ctrl('h'),
        ] {
            assert_eq!(event.clone().normalize_legacy(), event);
        }
    }

    #[test]
    fn normalize_preserves_kind_and_state() {
        // The Windows console reports a release for every key-up; a mapped
        // release must stay a release or the app acts on the key twice.
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            let out = ctrl('[')
                .with_kind(kind)
                .with_state(KeyEventState::KEYPAD)
                .normalize_legacy();
            assert_eq!(out.key, KeyCode::Esc);
            assert_eq!(out.kind, kind);
            assert_eq!(out.state, KeyEventState::KEYPAD);
        }
    }

    #[test]
    fn normalize_maps_ctrl_h_from_legacy_only() {
        let legacy = ctrl('h')
            .with_encoding(KeyEncoding::Legacy)
            .normalize_legacy();
        assert_eq!(legacy.key, KeyCode::Backspace);
        assert_eq!(legacy.modifiers, KeyModifiers::NONE);
        let xterm = ctrl('h').with_encoding(KeyEncoding::Xterm);
        assert_eq!(xterm.clone().normalize_legacy(), xterm);
    }

    // ── logical_char ────────────────────────────────────────────────────────

    fn logical(event: &KeyEvent) -> (Option<char>, KeyModifiers) {
        (event.logical_char(), event.logical_modifiers())
    }

    const ALT: KeyModifiers = KeyModifiers::ALT;
    const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
    const ALT_SHIFT: KeyModifiers = ALT.union(SHIFT);

    #[test]
    fn alt_shift_a_is_alt_capital_a_from_every_encoding() {
        // Kitty flag 1: `CSI 97 ; 4 u`, only the unshifted key.
        let flag1 = KeyEvent::new(KeyCode::Char('a'), ALT_SHIFT);
        // Kitty flag 4: `CSI 97 : 65 ; 4 u`.
        let flag4 = flag1.clone().with_shifted(Some('A'));
        // Legacy `ESC A` and xterm `CSI 27 ; 4 ; 65 ~` decode the same way.
        let legacy = flag4.clone().with_encoding(KeyEncoding::Legacy);
        let xterm = flag4.clone().with_encoding(KeyEncoding::Xterm);
        for event in [flag1, flag4, legacy, xterm] {
            assert_eq!(logical(&event), (Some('A'), ALT), "{event:?}");
        }
    }

    #[test]
    fn shift_stays_where_the_char_does_not_carry_it() {
        // Alt+Shift+1 under kitty flag 1 reports `1`; Shift is the only record.
        let event = KeyEvent::new(KeyCode::Char('1'), ALT_SHIFT);
        assert_eq!(logical(&event), (Some('1'), ALT_SHIFT));
        // Under flag 4 the shifted key says what it typed.
        let event = event.with_shifted(Some('!'));
        assert_eq!(logical(&event), (Some('!'), ALT));
    }

    #[test]
    fn non_ascii_letters_upper_case() {
        // Ctrl+Shift+é under kitty flag 1.
        let ctrl_shift = KeyModifiers::CONTROL | SHIFT;
        let event = KeyEvent::new(KeyCode::Char('é'), ctrl_shift);
        assert_eq!(logical(&event), (Some('É'), KeyModifiers::CONTROL));
        // ß has no single-character upper case: Shift stays.
        let event = KeyEvent::new(KeyCode::Char('ß'), SHIFT);
        assert_eq!(logical(&event), (Some('ß'), SHIFT));
    }

    #[test]
    fn text_wins_where_sent() {
        let event = KeyEvent::new(KeyCode::Char('a'), SHIFT).with_text(Some("A".into()));
        assert_eq!(logical(&event), (Some('A'), KeyModifiers::NONE));
        // Text that is the key itself carries no Shift.
        let event = KeyEvent::new(KeyCode::Char('1'), SHIFT).with_text(Some("1".into()));
        assert_eq!(logical(&event), (Some('1'), SHIFT));
        // Text of more than one character falls through to the key.
        let event =
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE).with_text(Some("ab".into()));
        assert_eq!(logical(&event), (Some('a'), KeyModifiers::NONE));
    }

    #[test]
    fn caps_lock_applies_to_plain_typing_only() {
        let caps = KeyEventState::CAPS_LOCK;
        let plain = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE).with_state(caps);
        assert_eq!(logical(&plain), (Some('A'), KeyModifiers::NONE));
        let shifted = KeyEvent::new(KeyCode::Char('a'), SHIFT).with_state(caps);
        assert_eq!(logical(&shifted), (Some('a'), KeyModifiers::NONE));
        let ctrl = ctrl('a').with_state(caps);
        assert_eq!(logical(&ctrl), (Some('a'), KeyModifiers::CONTROL));
    }

    #[test]
    fn non_char_keys_have_no_logical_char() {
        let event = KeyEvent::new(KeyCode::Up, SHIFT);
        assert_eq!(logical(&event), (None, SHIFT));
    }
}
