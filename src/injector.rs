use crate::error::{AppError, AppResult};
use arboard::Clipboard;
use enigo::{Enigo, Key, KeyboardControllable};
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowTarget(isize);

impl WindowTarget {
    pub fn capture_external() -> Option<Self> {
        let hwnd = unsafe { GetForegroundWindow() };
        let mut owner_process_id = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner_process_id);
        }
        external_target_for(hwnd, owner_process_id, unsafe { GetCurrentProcessId() })
    }

    fn hwnd(self) -> HWND {
        self.0 as HWND
    }

    #[cfg(test)]
    pub(crate) fn from_raw_for_test(value: isize) -> Self {
        Self(value)
    }
}

fn external_target_for(
    hwnd: HWND,
    owner_process_id: u32,
    current_process_id: u32,
) -> Option<WindowTarget> {
    if hwnd.is_null() || owner_process_id == 0 || owner_process_id == current_process_id {
        None
    } else {
        Some(WindowTarget(hwnd as isize))
    }
}

pub fn inject_text(
    target: Option<WindowTarget>,
    text: &str,
    restore_clipboard: bool,
) -> AppResult<()> {
    if text.trim().is_empty() {
        return Ok(());
    }
    // Missing targets must not turn into "paste into whatever is focused now".
    let target = target.ok_or_else(|| {
        AppError::Injection(
            "未保存有效的目標視窗，已取消自動貼上；辨識文字仍可手動複製".to_string(),
        )
    })?;
    focus_target(target)?;
    let mut clipboard = Clipboard::new().map_err(|e| AppError::Injection(e.to_string()))?;
    let previous = if restore_clipboard {
        clipboard.get_text().ok()
    } else {
        None
    };
    clipboard
        .set_text(text.to_string())
        .map_err(|e| AppError::Injection(e.to_string()))?;

    // Clipboard access may have taken time or activated another application.
    ensure_expected_foreground(target)?;
    let mut enigo = Enigo::new();
    enigo.key_down(Key::Control);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Control);
    thread::sleep(Duration::from_millis(120));

    if let Some(previous) = previous {
        // Best effort, not an atomic compare-and-swap. Preserve newer clipboard
        // content (and skip restoration when it cannot be inspected safely).
        let current = clipboard.get_text().ok();
        if clipboard_can_be_restored(current.as_deref(), text) {
            clipboard
                .set_text(previous)
                .map_err(|e| AppError::Injection(e.to_string()))?;
        }
    }
    Ok(())
}

fn focus_target(target: WindowTarget) -> AppResult<()> {
    if unsafe { IsWindow(target.hwnd()) } == 0 {
        return Err(AppError::Injection(
            "原本的焦點視窗已不存在，辨識文字仍可手動複製".to_string(),
        ));
    }
    let mut owner_process_id = 0;
    unsafe {
        GetWindowThreadProcessId(target.hwnd(), &mut owner_process_id);
    }
    if external_target_for(target.hwnd(), owner_process_id, unsafe {
        GetCurrentProcessId()
    }) != Some(target)
    {
        return Err(AppError::Injection(
            "目標視窗目前屬於 SpeakType 或已失效，為避免自動貼回程式本身已取消 Ctrl+V".to_string(),
        ));
    }
    if unsafe { SetForegroundWindow(target.hwnd()) } == 0 {
        return Err(AppError::Injection(
            "Windows 拒絕切換回原本的焦點視窗，辨識文字仍可手動複製".to_string(),
        ));
    }
    thread::sleep(Duration::from_millis(90));
    ensure_expected_foreground(target)
}

fn ensure_expected_foreground(target: WindowTarget) -> AppResult<()> {
    let actual = unsafe { GetForegroundWindow() };
    if !is_expected_foreground(target, actual) {
        return Err(AppError::Injection(
            "焦點在貼上前已離開原本視窗，為避免錯貼已取消 Ctrl+V；辨識文字仍可手動複製".to_string(),
        ));
    }
    Ok(())
}

fn is_expected_foreground(target: WindowTarget, actual: HWND) -> bool {
    !actual.is_null() && actual == target.hwnd()
}

fn clipboard_can_be_restored(current: Option<&str>, injected: &str) -> bool {
    current.is_some_and(|current| current == injected)
}

pub fn copy_text(text: &str) -> AppResult<()> {
    let mut clipboard = Clipboard::new().map_err(|e| AppError::Injection(e.to_string()))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| AppError::Injection(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_saved_window_is_rejected_before_injection() {
        let error = focus_target(WindowTarget(0))
            .expect_err("invalid window target must fail")
            .to_string();

        assert!(error.contains("視窗"));
    }

    #[test]
    fn missing_target_is_rejected_before_accessing_clipboard() {
        let error = inject_text(None, "需要保留的文字", true)
            .expect_err("missing target must never cause a paste")
            .to_string();
        assert!(error.contains("目標視窗"));
    }

    #[test]
    fn empty_text_does_not_require_a_target_or_touch_clipboard() {
        assert!(inject_text(None, " \n\t", true).is_ok());
    }

    #[test]
    fn restoration_is_allowed_only_while_injected_text_remains() {
        assert!(clipboard_can_be_restored(Some("辨識結果"), "辨識結果"));
        assert!(!clipboard_can_be_restored(
            Some("剛複製的新資料"),
            "辨識結果"
        ));
        assert!(!clipboard_can_be_restored(Some(""), "辨識結果"));
    }

    #[test]
    fn unreadable_or_non_text_clipboard_is_not_overwritten() {
        assert!(!clipboard_can_be_restored(None, "辨識結果"));
    }

    #[test]
    fn foreground_must_still_match_saved_target_after_wait() {
        let target = WindowTarget(42);

        assert!(is_expected_foreground(target, 42 as HWND));
        assert!(!is_expected_foreground(target, 7 as HWND));
        assert!(!is_expected_foreground(target, std::ptr::null_mut()));
    }

    #[test]
    fn current_process_windows_are_not_external_targets() {
        assert_eq!(
            external_target_for(42 as HWND, 200, 100),
            Some(WindowTarget(42))
        );
        assert_eq!(external_target_for(42 as HWND, 100, 100), None);
        assert_eq!(external_target_for(42 as HWND, 0, 100), None);
        assert_eq!(external_target_for(std::ptr::null_mut(), 200, 100), None);
    }
}
