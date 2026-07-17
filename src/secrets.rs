use crate::config::AppConfig;
use crate::error::{AppError, AppResult};

pub fn hydrate_process_environment(config: &AppConfig) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        for env_name in [&config.openai.api_key_env, &config.xai.api_key_env] {
            if !is_api_key_configured(env_name) {
                if let Some(api_key) = read_user_environment(env_name)? {
                    if !api_key.trim().is_empty() {
                        std::env::set_var(env_name, api_key);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = config;

    Ok(())
}

pub fn is_api_key_configured(env_name: &str) -> bool {
    std::env::var(env_name).is_ok_and(|value| !value.trim().is_empty())
}

pub fn save_api_key(env_name: &str, api_key: &str) -> AppResult<()> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::Configuration("API Key 不可為空".to_string()));
    }

    #[cfg(target_os = "windows")]
    {
        save_user_environment(env_name, api_key)?;
        std::env::set_var(env_name, api_key);
        broadcast_environment_change();
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (env_name, api_key);
        Err(AppError::Configuration(
            "GUI 儲存 API Key 目前僅支援 Windows".to_string(),
        ))
    }
}

pub fn clear_api_key(env_name: &str) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        clear_user_environment(env_name)?;
        std::env::remove_var(env_name);
        broadcast_environment_change();
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = env_name;
        Err(AppError::Configuration(
            "GUI 清除 API Key 目前僅支援 Windows".to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
fn read_user_environment(env_name: &str) -> AppResult<Option<String>> {
    use std::io::ErrorKind;
    use winreg::enums::KEY_READ;
    use winreg::HKCU;

    let environment = match HKCU.open_subkey_with_flags("Environment", KEY_READ) {
        Ok(environment) => environment,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::Io(format!(
                "無法讀取 Windows 使用者環境變數：{error}"
            )))
        }
    };

    match environment.get_value::<String, _>(env_name) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Io(format!(
            "無法讀取環境變數 {env_name}：{error}"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn save_user_environment(env_name: &str, api_key: &str) -> AppResult<()> {
    use winreg::HKCU;

    let (environment, _) = HKCU
        .create_subkey("Environment")
        .map_err(|error| AppError::Io(format!("無法開啟 Windows 使用者環境變數：{error}")))?;
    environment
        .set_value(env_name, &api_key)
        .map_err(|error| AppError::Io(format!("無法儲存環境變數 {env_name}：{error}")))
}

#[cfg(target_os = "windows")]
fn clear_user_environment(env_name: &str) -> AppResult<()> {
    use std::io::ErrorKind;
    use winreg::enums::KEY_SET_VALUE;
    use winreg::HKCU;

    let environment = match HKCU.open_subkey_with_flags("Environment", KEY_SET_VALUE) {
        Ok(environment) => environment,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(AppError::Io(format!(
                "無法開啟 Windows 使用者環境變數：{error}"
            )))
        }
    };

    match environment.delete_value(env_name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Io(format!(
            "無法清除環境變數 {env_name}：{error}"
        ))),
    }
}

#[cfg(target_os = "windows")]
fn broadcast_environment_change() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let environment = OsStr::new("Environment")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut result = 0_usize;

    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            environment.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            2_000,
            &mut result,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_key_requires_nonempty_value() {
        let variable = "SPEAKTYPE_TEST_GUI_KEY_STATUS";
        std::env::remove_var(variable);
        assert!(!is_api_key_configured(variable));

        std::env::set_var(variable, "   ");
        assert!(!is_api_key_configured(variable));

        std::env::set_var(variable, "test-secret");
        assert!(is_api_key_configured(variable));
        std::env::remove_var(variable);
    }
}
