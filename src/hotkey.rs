use device_query::{DeviceQuery, DeviceState, Keycode};
use rdev::{listen, Event, EventType, Key};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LISTENER_START_TIMEOUT: Duration = Duration::from_millis(250);
const IMMEDIATE_ERROR_WINDOW: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListenerStatus {
    ThreadStarted,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyCombo {
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
    key: String,
}

impl HotkeyCombo {
    fn parse(value: &str) -> Result<Self, String> {
        let mut combo = Self {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key: String::new(),
        };
        for token in value.split('+').map(str::trim).filter(|v| !v.is_empty()) {
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => combo.ctrl = true,
                "shift" => combo.shift = true,
                "alt" => combo.alt = true,
                "win" | "meta" | "windows" => combo.meta = true,
                key => combo.key = key.to_ascii_uppercase(),
            }
        }
        if combo.key.is_empty() {
            return Err("快捷鍵需要一個主鍵".to_string());
        }
        if !(combo.ctrl || combo.shift || combo.alt || combo.meta) {
            return Err("快捷鍵至少需要一個修飾鍵".to_string());
        }
        Ok(combo)
    }

    fn matches_raw(
        &self,
        ctrl_down: bool,
        shift_down: bool,
        alt_down: bool,
        meta_down: bool,
        pressed_keys: &HashSet<String>,
    ) -> bool {
        (!self.ctrl || ctrl_down)
            && (!self.shift || shift_down)
            && (!self.alt || alt_down)
            && (!self.meta || meta_down)
            && pressed_keys.contains(&self.key)
    }
}

#[derive(Default)]
struct KeyboardState {
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
    keys: HashSet<String>,
}

/// Global hotkey detector.
///
/// Uses a **dual‑crate** approach for maximum reliability:
///   1. `rdev::listen` — event‑driven callback pushes `HotkeyEvent`s to a
///      `VecDeque` and also stores the current match state in an `AtomicBool`.
///   2. `device_query::DeviceState::get_keys` — synchronous Windows‑API polling
///      fallback when `rdev` misses events (e.g. under heavy load, UIPI delay).
///
/// `poll_event` drains the queue first; if empty it falls back to `device_query`
/// polling with edge‑detection so no Pressed/Released event is ever lost.
pub struct GlobalHotkey {
    combo: Arc<Mutex<HotkeyCombo>>,
    events: Arc<Mutex<VecDeque<HotkeyEvent>>>,
    status_rx: Receiver<ListenerStatus>,
    device_state: DeviceState,
    last_hotkey_pressed: bool,
    hotkey_pressed: Arc<AtomicBool>,
}

impl GlobalHotkey {
    pub fn new(value: &str) -> Result<Self, String> {
        let combo = Arc::new(Mutex::new(HotkeyCombo::parse(value)?));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let hotkey_pressed = Arc::new(AtomicBool::new(false));
        let combo_thread = combo.clone();
        let events_thread = events.clone();
        let hotkey_clone = hotkey_pressed.clone();
        let (status_tx, status_rx) = mpsc::channel();

        std::thread::spawn(move || {
            if status_tx.send(ListenerStatus::ThreadStarted).is_err() {
                return;
            }
            let mut state = KeyboardState::default();
            let mut was_down = false;
            let result = listen(move |event: Event| {
                apply_event(&mut state, event.event_type);
                // Short‑circuit on mutex contention (unwrap_or) so we never
                // block the keyboard hook — a missed detection is recoverable.
                let is_down = combo_thread
                    .lock()
                    .map(|combo| {
                        combo.matches_raw(
                            state.ctrl,
                            state.shift,
                            state.alt,
                            state.meta,
                            &state.keys,
                        )
                    })
                    .unwrap_or(false);
                if is_down != was_down {
                    was_down = is_down;
                    if let Ok(mut queue) = events_thread.lock() {
                        queue.push_back(if is_down {
                            HotkeyEvent::Pressed
                        } else {
                            HotkeyEvent::Released
                        });
                    }
                }
                // Fast path for the polling fallback.
                hotkey_clone.store(is_down, Ordering::Relaxed);
            });
            let message = match result {
                Ok(()) => "全域快捷鍵 listener 意外停止".to_string(),
                Err(error) => format!("全域快捷鍵 listener 啟動或執行失敗：{error:?}"),
            };
            let _ = status_tx.send(ListenerStatus::Failed(message));
        });

        startup_handshake(&status_rx, LISTENER_START_TIMEOUT, IMMEDIATE_ERROR_WINDOW)?;

        Ok(Self {
            combo,
            events,
            status_rx,
            device_state: DeviceState::new(),
            last_hotkey_pressed: false,
            hotkey_pressed,
        })
    }

    pub fn update(&self, value: &str) -> Result<(), String> {
        let parsed = HotkeyCombo::parse(value)?;
        let mut combo = self
            .combo
            .lock()
            .map_err(|_| "快捷鍵鎖定失敗".to_string())?;
        *combo = parsed;
        Ok(())
    }

    /// Poll for a hotkey event using a dual‑crate approach:
    ///
    /// 1. **rdev event queue** — drain any queued `HotkeyEvent`s.
    /// 2. **device_query fallback** — if the queue is empty, synchronously
    ///    poll the current keyboard state and compare against the last known
    ///    state to synthesise edge‑triggered Pressed/Released events.
    ///
    /// This guarantees that even if `rdev` temporarily misses events (common
    /// under UIPI, heavy CPU load, or thread scheduling delays), the hotkey
    /// is still reliably detected.
    pub fn poll_event(&mut self) -> Option<HotkeyEvent> {
        // Try the rdev event queue first — fastest path.
        if let Some(event) = self
            .events
            .lock()
            .map(|mut queue| queue.pop_front())
            .unwrap_or_else(|e| e.into_inner().pop_front())
        {
            self.last_hotkey_pressed = matches!(event, HotkeyEvent::Pressed);
            return Some(event);
        }

        // No rdev events — fall back to synchronous polling.
        let pressed = self.is_hotkey_down();

        let event = if pressed && !self.last_hotkey_pressed {
            Some(HotkeyEvent::Pressed)
        } else if !pressed && self.last_hotkey_pressed {
            Some(HotkeyEvent::Released)
        } else {
            None
        };

        self.last_hotkey_pressed = pressed;
        event
    }

    /// Check whether the configured hotkey is currently held down.
    ///
    /// Fast path  — `AtomicBool` set by the `rdev` callback (lock‑free).
    /// Slow path  — `device_query::DeviceState::get_keys()` synchronous poll.
    fn is_hotkey_down(&self) -> bool {
        // Fast path: rdev already told us the combo is pressed.
        if self.hotkey_pressed.load(Ordering::Relaxed) {
            return true;
        }

        // Slow path: query the current keyboard state via Windows API.
        let keys: Vec<Keycode> = self.device_state.get_keys();
        let ctrl_down = keys.contains(&Keycode::LControl) || keys.contains(&Keycode::RControl);
        let shift_down = keys.contains(&Keycode::LShift) || keys.contains(&Keycode::RShift);
        let alt_down = keys.contains(&Keycode::LAlt) || keys.contains(&Keycode::RAlt);
        let meta_down = keys.contains(&Keycode::LMeta) || keys.contains(&Keycode::RMeta);
        let pressed_keys: HashSet<String> = keys
            .iter()
            .filter_map(|k| normalize_device_key(*k))
            .collect();

        self.combo
            .lock()
            .map(|combo| {
                combo.matches_raw(ctrl_down, shift_down, alt_down, meta_down, &pressed_keys)
            })
            .unwrap_or(false)
    }

    pub fn poll_error(&self) -> Option<String> {
        poll_listener_error(&self.status_rx)
    }
}

fn startup_handshake(
    receiver: &Receiver<ListenerStatus>,
    start_timeout: Duration,
    immediate_error_window: Duration,
) -> Result<(), String> {
    match receiver.recv_timeout(start_timeout) {
        Ok(ListenerStatus::ThreadStarted) => {}
        Ok(ListenerStatus::Failed(error)) => return Err(error),
        Err(RecvTimeoutError::Timeout) => {
            return Err("全域快捷鍵 listener 啟動逾時".to_string());
        }
        Err(RecvTimeoutError::Disconnected) => {
            return Err("全域快捷鍵 listener 啟動前已中止".to_string());
        }
    }

    match receiver.recv_timeout(immediate_error_window) {
        Ok(ListenerStatus::Failed(error)) => Err(error),
        Ok(ListenerStatus::ThreadStarted) | Err(RecvTimeoutError::Timeout) => Ok(()),
        Err(RecvTimeoutError::Disconnected) => {
            Err("全域快捷鍵 listener 啟動後立即中止".to_string())
        }
    }
}

fn poll_listener_error(receiver: &Receiver<ListenerStatus>) -> Option<String> {
    match receiver.try_recv() {
        Ok(ListenerStatus::Failed(error)) => Some(error),
        Ok(ListenerStatus::ThreadStarted) | Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some("全域快捷鍵 listener 已中止".to_string()),
    }
}

fn apply_event(state: &mut KeyboardState, event: EventType) {
    match event {
        EventType::KeyPress(Key::ControlLeft | Key::ControlRight) => state.ctrl = true,
        EventType::KeyRelease(Key::ControlLeft | Key::ControlRight) => state.ctrl = false,
        EventType::KeyPress(Key::ShiftLeft | Key::ShiftRight) => state.shift = true,
        EventType::KeyRelease(Key::ShiftLeft | Key::ShiftRight) => state.shift = false,
        EventType::KeyPress(Key::Alt | Key::AltGr) => state.alt = true,
        EventType::KeyRelease(Key::Alt | Key::AltGr) => state.alt = false,
        EventType::KeyPress(Key::MetaLeft | Key::MetaRight) => state.meta = true,
        EventType::KeyRelease(Key::MetaLeft | Key::MetaRight) => state.meta = false,
        EventType::KeyPress(key) => {
            if let Some(name) = key_name(key) {
                state.keys.insert(name);
            }
        }
        EventType::KeyRelease(key) => {
            if let Some(name) = key_name(key) {
                state.keys.remove(&name);
            }
        }
        _ => {}
    }
}

fn key_name(key: Key) -> Option<String> {
    match key {
        Key::Space => Some("SPACE".to_string()),
        Key::Return => Some("ENTER".to_string()),
        Key::Tab => Some("TAB".to_string()),
        Key::Escape => Some("ESC".to_string()),
        Key::KeyA => Some("A".to_string()),
        Key::KeyB => Some("B".to_string()),
        Key::KeyC => Some("C".to_string()),
        Key::KeyD => Some("D".to_string()),
        Key::KeyE => Some("E".to_string()),
        Key::KeyF => Some("F".to_string()),
        Key::KeyG => Some("G".to_string()),
        Key::KeyH => Some("H".to_string()),
        Key::KeyI => Some("I".to_string()),
        Key::KeyJ => Some("J".to_string()),
        Key::KeyK => Some("K".to_string()),
        Key::KeyL => Some("L".to_string()),
        Key::KeyM => Some("M".to_string()),
        Key::KeyN => Some("N".to_string()),
        Key::KeyO => Some("O".to_string()),
        Key::KeyP => Some("P".to_string()),
        Key::KeyQ => Some("Q".to_string()),
        Key::KeyR => Some("R".to_string()),
        Key::KeyS => Some("S".to_string()),
        Key::KeyT => Some("T".to_string()),
        Key::KeyU => Some("U".to_string()),
        Key::KeyV => Some("V".to_string()),
        Key::KeyW => Some("W".to_string()),
        Key::KeyX => Some("X".to_string()),
        Key::KeyY => Some("Y".to_string()),
        Key::KeyZ => Some("Z".to_string()),
        _ => None,
    }
}

/// Canonical key string for [`device_query::Keycode`].
/// Matches the naming convention of [`key_name`] so both can be compared
/// against the same `HotkeyCombo.key` field.
fn normalize_device_key(key: Keycode) -> Option<String> {
    match key {
        Keycode::A => Some("A"),
        Keycode::B => Some("B"),
        Keycode::C => Some("C"),
        Keycode::D => Some("D"),
        Keycode::E => Some("E"),
        Keycode::F => Some("F"),
        Keycode::G => Some("G"),
        Keycode::H => Some("H"),
        Keycode::I => Some("I"),
        Keycode::J => Some("J"),
        Keycode::K => Some("K"),
        Keycode::L => Some("L"),
        Keycode::M => Some("M"),
        Keycode::N => Some("N"),
        Keycode::O => Some("O"),
        Keycode::P => Some("P"),
        Keycode::Q => Some("Q"),
        Keycode::R => Some("R"),
        Keycode::S => Some("S"),
        Keycode::T => Some("T"),
        Keycode::U => Some("U"),
        Keycode::V => Some("V"),
        Keycode::W => Some("W"),
        Keycode::X => Some("X"),
        Keycode::Y => Some("Y"),
        Keycode::Z => Some("Z"),
        Keycode::Key0 => Some("0"),
        Keycode::Key1 => Some("1"),
        Keycode::Key2 => Some("2"),
        Keycode::Key3 => Some("3"),
        Keycode::Key4 => Some("4"),
        Keycode::Key5 => Some("5"),
        Keycode::Key6 => Some("6"),
        Keycode::Key7 => Some("7"),
        Keycode::Key8 => Some("8"),
        Keycode::Key9 => Some("9"),
        Keycode::F1 => Some("F1"),
        Keycode::F2 => Some("F2"),
        Keycode::F3 => Some("F3"),
        Keycode::F4 => Some("F4"),
        Keycode::F5 => Some("F5"),
        Keycode::F6 => Some("F6"),
        Keycode::F7 => Some("F7"),
        Keycode::F8 => Some("F8"),
        Keycode::F9 => Some("F9"),
        Keycode::F10 => Some("F10"),
        Keycode::F11 => Some("F11"),
        Keycode::F12 => Some("F12"),
        Keycode::Space => Some("SPACE"),
        Keycode::Enter => Some("ENTER"),
        Keycode::Tab => Some("TAB"),
        Keycode::Escape => Some("ESC"),
        _ => None,
    }
    .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn parses_default_hotkey() {
        let hotkey = HotkeyCombo::parse("Ctrl+Shift+Space").expect("parse");
        assert!(hotkey.ctrl && hotkey.shift);
        assert_eq!(hotkey.key, "SPACE");
    }

    #[test]
    fn startup_handshake_accepts_started_without_immediate_error() {
        let (tx, rx) = mpsc::channel();
        tx.send(ListenerStatus::ThreadStarted)
            .expect("started status");

        assert!(startup_handshake(&rx, Duration::ZERO, Duration::ZERO).is_ok());
    }

    #[test]
    fn startup_handshake_reports_immediate_listener_failure() {
        let (tx, rx) = mpsc::channel();
        tx.send(ListenerStatus::ThreadStarted)
            .expect("started status");
        tx.send(ListenerStatus::Failed("hook denied".to_string()))
            .expect("failure status");

        let error = startup_handshake(&rx, Duration::ZERO, Duration::ZERO)
            .expect_err("immediate failure must fail startup");

        assert!(error.contains("hook denied"));
    }

    #[test]
    fn startup_handshake_is_bounded_when_thread_never_reports_started() {
        let (_tx, rx) = mpsc::channel();

        let error = startup_handshake(&rx, Duration::ZERO, Duration::ZERO)
            .expect_err("missing startup status must time out");

        assert!(error.contains("逾時"));
    }

    #[test]
    fn runtime_failure_is_exposed_to_ui_consumer() {
        let (tx, rx) = mpsc::channel();
        tx.send(ListenerStatus::Failed("listener stopped".to_string()))
            .expect("runtime status");

        assert_eq!(
            poll_listener_error(&rx),
            Some("listener stopped".to_string())
        );
    }

    #[test]
    fn matches_raw_requires_all_modifiers_and_key() {
        let combo = HotkeyCombo {
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
            key: "A".to_string(),
        };
        let mut keys = HashSet::new();
        keys.insert("A".to_string());
        assert!(combo.matches_raw(true, false, false, false, &keys));
        assert!(!combo.matches_raw(false, false, false, false, &keys));
        assert!(!combo.matches_raw(true, false, false, false, &HashSet::new()));
    }

    #[test]
    fn matches_raw_ignores_unrequired_modifiers() {
        let combo = HotkeyCombo {
            ctrl: true,
            shift: true,
            alt: false,
            meta: false,
            key: "L".to_string(),
        };
        let mut keys = HashSet::new();
        keys.insert("L".to_string());
        // Alt is not required, so it should still match when alt is down.
        assert!(combo.matches_raw(true, true, true, false, &keys));
    }

    #[test]
    fn normalize_device_key_roundtrip_letter() {
        assert_eq!(normalize_device_key(Keycode::A), Some("A".to_string()));
        assert_eq!(normalize_device_key(Keycode::Z), Some("Z".to_string()));
        assert_eq!(
            normalize_device_key(Keycode::Space),
            Some("SPACE".to_string())
        );
        assert_eq!(
            normalize_device_key(Keycode::Enter),
            Some("ENTER".to_string())
        );
    }

    #[test]
    fn normalize_device_key_unknown_returns_none() {
        assert_eq!(normalize_device_key(Keycode::CapsLock), None);
        assert_eq!(normalize_device_key(Keycode::Insert), None);
    }
}
