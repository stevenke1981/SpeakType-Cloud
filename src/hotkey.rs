use rdev::{listen, Event, EventType, Key};
use std::collections::{HashSet, VecDeque};
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

    fn matches(&self, state: &KeyboardState) -> bool {
        (!self.ctrl || state.ctrl)
            && (!self.shift || state.shift)
            && (!self.alt || state.alt)
            && (!self.meta || state.meta)
            && state.keys.contains(&self.key)
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

pub struct GlobalHotkey {
    combo: Arc<Mutex<HotkeyCombo>>,
    events: Arc<Mutex<VecDeque<HotkeyEvent>>>,
    status_rx: Receiver<ListenerStatus>,
}

impl GlobalHotkey {
    pub fn new(value: &str) -> Result<Self, String> {
        let combo = Arc::new(Mutex::new(HotkeyCombo::parse(value)?));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let combo_thread = combo.clone();
        let events_thread = events.clone();
        let (status_tx, status_rx) = mpsc::channel();

        std::thread::spawn(move || {
            if status_tx.send(ListenerStatus::ThreadStarted).is_err() {
                return;
            }
            let mut state = KeyboardState::default();
            let mut was_down = false;
            let result = listen(move |event: Event| {
                apply_event(&mut state, event.event_type);
                let is_down = combo_thread
                    .lock()
                    .map(|combo| combo.matches(&state))
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

    pub fn poll(&self) -> Option<HotkeyEvent> {
        self.events.lock().ok()?.pop_front()
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
}
