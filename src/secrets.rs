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
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ,
    };

    let subkey = wide("Environment");
    let value_name = wide(env_name);
    let mut byte_count = 0_u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut byte_count,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(registry_error(
            &format!("無法讀取環境變數 {env_name}"),
            status,
        ));
    }
    if byte_count == 0 {
        return Ok(Some(String::new()));
    }

    let mut data = vec![0_u16; (byte_count as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast::<c_void>(),
            &mut byte_count,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(registry_error(
            &format!("無法讀取環境變數 {env_name}"),
            status,
        ));
    }

    data.truncate((byte_count as usize / 2).min(data.len()));
    while data.last() == Some(&0) {
        data.pop();
    }
    String::from_utf16(&data)
        .map(Some)
        .map_err(|error| AppError::Io(format!("環境變數 {env_name} 不是有效的 Unicode：{error}")))
}

#[cfg(target_os = "windows")]
fn save_user_environment(env_name: &str, api_key: &str) -> AppResult<()> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    };

    let subkey = wide("Environment");
    let value_name = wide(env_name);
    let data = wide(api_key);
    let byte_count = u32::try_from(data.len().saturating_mul(std::mem::size_of::<u16>()))
        .map_err(|_| AppError::Configuration("API Key 長度超出 Windows 限制".to_string()))?;
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
            REG_SZ,
            data.as_ptr().cast::<c_void>(),
            byte_count,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(registry_error(
            &format!("無法儲存環境變數 {env_name}"),
            status,
        ))
    }
}

#[cfg(target_os = "windows")]
fn clear_user_environment(env_name: &str) -> AppResult<()> {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, HKEY_CURRENT_USER,
    };

    let subkey = wide("Environment");
    let value_name = wide(env_name);
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value_name.as_ptr(),
        )
    };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(registry_error(
            &format!("無法清除環境變數 {env_name}"),
            status,
        ))
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

#[cfg(target_os = "windows")]
fn registry_error(context: &str, status: u32) -> AppError {
    let error = std::io::Error::from_raw_os_error(status as i32);
    AppError::Io(format!("{context}：{error}"))
}

#[cfg(target_os = "windows")]
fn broadcast_environment_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    let environment = wide("Environment");
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
