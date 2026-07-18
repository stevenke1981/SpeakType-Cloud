use crate::config::AppConfig;
use crate::error::{AppError, AppResult};

trait SecretStore {
    fn read_secret(&self, env_name: &str) -> AppResult<Option<String>>;
    fn write_secret(&self, env_name: &str, api_key: &str) -> AppResult<()>;
    fn delete_secret(&self, env_name: &str) -> AppResult<()>;
    fn read_legacy_environment(&self, env_name: &str) -> AppResult<Option<String>>;
}

#[cfg(target_os = "windows")]
struct WindowsSecretStore;

pub fn hydrate_process_environment(config: &AppConfig) -> AppResult<()> {
    config.validate()?;
    #[cfg(target_os = "windows")]
    {
        let store = WindowsSecretStore;
        for env_name in [&config.openai.api_key_env, &config.xai.api_key_env] {
            hydrate_key_with_store(&store, env_name, legacy_migration_allowed(env_name))?;
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
        let store = WindowsSecretStore;
        store.write_secret(env_name, api_key)?;
        std::env::set_var(env_name, api_key);
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
        let store = WindowsSecretStore;
        store.delete_secret(env_name)?;
        std::env::remove_var(env_name);
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

fn legacy_migration_allowed(env_name: &str) -> bool {
    matches!(env_name, "OPENAI_API_KEY" | "XAI_API_KEY")
}

fn hydrate_key_with_store(
    store: &impl SecretStore,
    env_name: &str,
    migrate_legacy: bool,
) -> AppResult<()> {
    let mut stored = store
        .read_secret(env_name)?
        .filter(|value| !value.trim().is_empty());
    if migrate_legacy {
        if let Some(legacy) = store
            .read_legacy_environment(env_name)?
            .filter(|value| !value.trim().is_empty())
        {
            if stored.is_none() {
                store.write_secret(env_name, &legacy)?;
                stored = Some(legacy);
            }
        }
    }

    if !is_api_key_configured(env_name) {
        if let Some(api_key) = stored {
            std::env::set_var(env_name, api_key);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
impl SecretStore for WindowsSecretStore {
    fn read_secret(&self, env_name: &str) -> AppResult<Option<String>> {
        read_windows_credential(env_name)
    }

    fn write_secret(&self, env_name: &str, api_key: &str) -> AppResult<()> {
        write_windows_credential(env_name, api_key)
    }

    fn delete_secret(&self, env_name: &str) -> AppResult<()> {
        delete_windows_credential(env_name)
    }

    fn read_legacy_environment(&self, env_name: &str) -> AppResult<Option<String>> {
        read_user_environment(env_name)
    }
}

#[cfg(target_os = "windows")]
fn credential_target(env_name: &str) -> String {
    format!("SpeakTypeCloud:{env_name}")
}

#[cfg(target_os = "windows")]
fn read_windows_credential(env_name: &str) -> AppResult<Option<String>> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = wide(&credential_target(env_name));
    let mut credential = std::ptr::null_mut::<CREDENTIALW>();
    let success = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if success == 0 {
        let status = unsafe { GetLastError() };
        return if status == ERROR_NOT_FOUND {
            Ok(None)
        } else {
            Err(credential_error(
                "無法讀取 Windows Credential Manager",
                status,
            ))
        };
    }

    let credential_ref = unsafe { &*credential };
    let bytes = if credential_ref.CredentialBlobSize == 0 {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(
                credential_ref.CredentialBlob,
                credential_ref.CredentialBlobSize as usize,
            )
        }
        .to_vec()
    };
    unsafe { CredFree(credential.cast()) };

    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| AppError::Io("Windows Credential Manager 內的 API Key 編碼無效".to_string()))
}

#[cfg(target_os = "windows")]
fn write_windows_credential(env_name: &str, api_key: &str) -> AppResult<()> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let target = wide(&credential_target(env_name));
    let user_name = wide("SpeakType Cloud");
    let mut blob = api_key.as_bytes().to_vec();
    let blob_size = u32::try_from(blob.len())
        .map_err(|_| AppError::Configuration("API Key 長度超出 Windows 限制".to_string()))?;
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr().cast_mut(),
        CredentialBlobSize: blob_size,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user_name.as_ptr().cast_mut(),
        ..CREDENTIALW::default()
    };
    let success = unsafe { CredWriteW(&credential, 0) };
    let status = (success == 0).then(|| unsafe { GetLastError() });
    blob.fill(0);
    if let Some(status) = status {
        Err(credential_error(
            "無法寫入 Windows Credential Manager",
            status,
        ))
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn delete_windows_credential(env_name: &str) -> AppResult<()> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide(&credential_target(env_name));
    let success = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if success != 0 {
        return Ok(());
    }
    let status = unsafe { GetLastError() };
    if status == ERROR_NOT_FOUND {
        Ok(())
    } else {
        Err(credential_error(
            "無法清除 Windows Credential Manager",
            status,
        ))
    }
}

#[cfg(target_os = "windows")]
fn read_user_environment(env_name: &str) -> AppResult<Option<String>> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

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
fn credential_error(context: &str, status: u32) -> AppError {
    let error = std::io::Error::from_raw_os_error(status as i32);
    AppError::Io(format!("{context}：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct MemorySecretStore {
        secret: RefCell<Option<String>>,
        legacy: RefCell<Option<String>>,
        fail_write: Cell<bool>,
        legacy_reads: Cell<usize>,
    }

    impl SecretStore for MemorySecretStore {
        fn read_secret(&self, _env_name: &str) -> AppResult<Option<String>> {
            Ok(self.secret.borrow().clone())
        }

        fn write_secret(&self, _env_name: &str, api_key: &str) -> AppResult<()> {
            if self.fail_write.get() {
                return Err(AppError::Io(
                    "injected credential write failure".to_string(),
                ));
            }
            self.secret.replace(Some(api_key.to_string()));
            Ok(())
        }

        fn delete_secret(&self, _env_name: &str) -> AppResult<()> {
            self.secret.replace(None);
            Ok(())
        }

        fn read_legacy_environment(&self, _env_name: &str) -> AppResult<Option<String>> {
            self.legacy_reads.set(self.legacy_reads.get() + 1);
            Ok(self.legacy.borrow().clone())
        }
    }

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

    #[test]
    fn standard_key_is_imported_without_changing_external_legacy_value() {
        assert!(legacy_migration_allowed("OPENAI_API_KEY"));
        assert!(legacy_migration_allowed("XAI_API_KEY"));
        let variable = "SPEAKTYPE_TEST_LEGACY_SECRET_MIGRATION";
        let store = MemorySecretStore {
            secret: RefCell::new(None),
            legacy: RefCell::new(Some("legacy-test-secret".to_string())),
            ..MemorySecretStore::default()
        };
        std::env::remove_var(variable);

        hydrate_key_with_store(&store, variable, true).expect("migrate legacy secret");

        assert_eq!(store.secret.borrow().as_deref(), Some("legacy-test-secret"));
        assert_eq!(store.legacy.borrow().as_deref(), Some("legacy-test-secret"));
        assert_eq!(std::env::var(variable).as_deref(), Ok("legacy-test-secret"));
        std::env::remove_var(variable);
    }

    #[test]
    fn explicit_process_environment_secret_has_priority() {
        let variable = "SPEAKTYPE_TEST_PROCESS_SECRET_PRIORITY";
        let store = MemorySecretStore {
            secret: RefCell::new(Some("stored-test-secret".to_string())),
            legacy: RefCell::new(None),
            ..MemorySecretStore::default()
        };
        std::env::set_var(variable, "process-test-secret");

        hydrate_key_with_store(&store, variable, false).expect("hydrate secret");

        assert_eq!(
            std::env::var(variable).as_deref(),
            Ok("process-test-secret")
        );
        std::env::remove_var(variable);
    }

    #[test]
    fn path_never_reads_or_deletes_legacy_registry_value() {
        let store = MemorySecretStore {
            legacy: RefCell::new(Some("must-not-touch".to_string())),
            ..MemorySecretStore::default()
        };

        hydrate_key_with_store(&store, "PATH", legacy_migration_allowed("PATH"))
            .expect("hydrate custom credential name");

        assert_eq!(store.legacy_reads.get(), 0);
        assert_eq!(store.legacy.borrow().as_deref(), Some("must-not-touch"));
    }

    #[test]
    fn empty_credential_is_replaced_without_changing_legacy_value() {
        let variable = "SPEAKTYPE_TEST_EMPTY_CREDENTIAL_MIGRATION";
        let store = MemorySecretStore {
            secret: RefCell::new(Some("   ".to_string())),
            legacy: RefCell::new(Some("legacy-test-secret".to_string())),
            ..MemorySecretStore::default()
        };
        std::env::remove_var(variable);

        hydrate_key_with_store(&store, variable, true).expect("replace empty credential");

        assert_eq!(store.secret.borrow().as_deref(), Some("legacy-test-secret"));
        assert_eq!(store.legacy.borrow().as_deref(), Some("legacy-test-secret"));
        std::env::remove_var(variable);
    }

    #[test]
    fn credential_write_failure_keeps_legacy_value() {
        let variable = "SPEAKTYPE_TEST_FAILED_SECRET_MIGRATION";
        let store = MemorySecretStore {
            legacy: RefCell::new(Some("legacy-test-secret".to_string())),
            fail_write: Cell::new(true),
            ..MemorySecretStore::default()
        };
        std::env::remove_var(variable);

        assert!(hydrate_key_with_store(&store, variable, true).is_err());
        assert_eq!(store.legacy.borrow().as_deref(), Some("legacy-test-secret"));
    }
}
