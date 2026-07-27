#[cfg(target_os = "windows")]
pub struct SingleInstance {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl SingleInstance {
    pub fn acquire() -> Result<Self, String> {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
        use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex};

        let name = crate::win_util::wide("Local\\SpeakTypeCloud_SingleInstance");
        let handle: HANDLE = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if handle.is_null() {
            let status = unsafe { GetLastError() };
            return Err(format!("無法建立單實例 Mutex（錯誤碼 {status}）"));
        }
        let status = unsafe { GetLastError() };
        if status == ERROR_ALREADY_EXISTS {
            unsafe { ReleaseMutex(handle) };
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
            return Err("SpeakType Cloud 已在執行中".to_string());
        }
        Ok(Self { handle })
    }
}

#[cfg(target_os = "windows")]
impl Drop for SingleInstance {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ReleaseMutex;

        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub struct SingleInstance;

#[cfg(not(target_os = "windows"))]
impl SingleInstance {
    pub fn acquire() -> Result<Self, String> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_instance_can_be_acquired() {
        let instance = SingleInstance::acquire();
        assert!(instance.is_ok());
    }
}
