use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use std::path::Path;

const RUN_VALUE_NAME: &str = "SpeakType Cloud";

trait SettingsStore {
    fn save(&self, config: &AppConfig) -> AppResult<()>;
}

trait LaunchAtLoginStore {
    fn set_enabled(&self, enabled: bool) -> AppResult<()>;
}

struct DiskSettingsStore;
struct SystemLaunchAtLoginStore;

impl SettingsStore for DiskSettingsStore {
    fn save(&self, config: &AppConfig) -> AppResult<()> {
        config.save()
    }
}

impl LaunchAtLoginStore for SystemLaunchAtLoginStore {
    fn set_enabled(&self, enabled: bool) -> AppResult<()> {
        set_launch_at_login(enabled)
    }
}

pub fn persist_config(previous: &AppConfig, next: &AppConfig) -> AppResult<()> {
    persist_with_stores(
        previous,
        next,
        &DiskSettingsStore,
        &SystemLaunchAtLoginStore,
    )
}

fn persist_with_stores(
    previous: &AppConfig,
    next: &AppConfig,
    settings: &impl SettingsStore,
    launch: &impl LaunchAtLoginStore,
) -> AppResult<()> {
    settings.save(next)?;
    if previous.launch_at_login == next.launch_at_login {
        return Ok(());
    }
    if let Err(registry_error) = launch.set_enabled(next.launch_at_login) {
        return match settings.save(previous) {
            Ok(()) => Err(registry_error),
            Err(rollback_error) => Err(AppError::Io(format!(
                "{registry_error}；設定回滾也失敗：{rollback_error}"
            ))),
        };
    }
    Ok(())
}

pub fn set_launch_at_login(enabled: bool) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        set_windows_launch_at_login(enabled)
    }

    #[cfg(not(target_os = "windows"))]
    {
        if enabled {
            Err(AppError::Configuration(
                "登入自動啟動目前僅支援 Windows".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

fn quote_executable_command(path: &Path) -> AppResult<String> {
    let path = path
        .to_str()
        .ok_or_else(|| AppError::Configuration("程式路徑不是有效的 Windows Unicode".to_string()))?;
    if path.is_empty() || path.contains('"') {
        return Err(AppError::Configuration(
            "程式路徑無法安全寫入登入啟動設定".to_string(),
        ));
    }
    Ok(format!("\"{path}\""))
}

#[cfg(target_os = "windows")]
fn set_windows_launch_at_login(enabled: bool) -> AppResult<()> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    };

    let subkey = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let value_name = wide(RUN_VALUE_NAME);
    let status = if enabled {
        let executable = std::env::current_exe()
            .map_err(|error| AppError::Io(format!("無法取得目前程式路徑：{error}")))?;
        let command = quote_executable_command(&executable)?;
        let data = wide(&command);
        let byte_count = u32::try_from(data.len().saturating_mul(std::mem::size_of::<u16>()))
            .map_err(|_| AppError::Configuration("登入啟動命令長度超出限制".to_string()))?;
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                value_name.as_ptr(),
                REG_SZ,
                data.as_ptr().cast::<c_void>(),
                byte_count,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value_name.as_ptr()) }
    };

    if status == ERROR_SUCCESS || (!enabled && status == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        let error = std::io::Error::from_raw_os_error(status as i32);
        Err(AppError::Io(format!(
            "無法{} Windows 登入自動啟動：{error}",
            if enabled { "啟用" } else { "停用" }
        )))
    }
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct MemorySettingsStore {
        writes: RefCell<Vec<bool>>,
        fail_next: Cell<bool>,
    }

    impl SettingsStore for MemorySettingsStore {
        fn save(&self, config: &AppConfig) -> AppResult<()> {
            self.writes.borrow_mut().push(config.launch_at_login);
            if self.fail_next.replace(false) {
                Err(AppError::Io("injected config write failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct MemoryLaunchStore {
        writes: RefCell<Vec<bool>>,
        fail: Cell<bool>,
    }

    impl LaunchAtLoginStore for MemoryLaunchStore {
        fn set_enabled(&self, enabled: bool) -> AppResult<()> {
            self.writes.borrow_mut().push(enabled);
            if self.fail.get() {
                Err(AppError::Io("injected registry failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn executable_path_is_quoted_for_hkcu_run() {
        let command = quote_executable_command(Path::new(
            r"C:\Program Files\SpeakType Cloud\speaktype-cloud.exe",
        ))
        .expect("quote executable path");

        assert_eq!(
            command,
            r#""C:\Program Files\SpeakType Cloud\speaktype-cloud.exe""#
        );
    }

    #[test]
    fn executable_path_with_quote_is_rejected() {
        assert!(quote_executable_command(Path::new(r#"C:\bad"path\app.exe"#)).is_err());
    }

    #[test]
    fn config_failure_does_not_change_launch_registry() {
        let previous = AppConfig::default();
        let mut next = previous.clone();
        next.launch_at_login = true;
        let settings = MemorySettingsStore {
            fail_next: Cell::new(true),
            ..MemorySettingsStore::default()
        };
        let launch = MemoryLaunchStore::default();

        assert!(persist_with_stores(&previous, &next, &settings, &launch).is_err());
        assert_eq!(&*settings.writes.borrow(), &[true]);
        assert!(launch.writes.borrow().is_empty());
    }

    #[test]
    fn registry_failure_rolls_config_back_to_previous_value() {
        let previous = AppConfig::default();
        let mut next = previous.clone();
        next.launch_at_login = true;
        let settings = MemorySettingsStore::default();
        let launch = MemoryLaunchStore {
            fail: Cell::new(true),
            ..MemoryLaunchStore::default()
        };

        assert!(persist_with_stores(&previous, &next, &settings, &launch).is_err());
        assert_eq!(&*settings.writes.borrow(), &[true, false]);
        assert_eq!(&*launch.writes.borrow(), &[true]);
    }
}
