use std::ffi::c_void;
use windows::{
    core::*, Win32::Foundation::*, Win32::System::LibraryLoader::*,
    Win32::System::SystemServices::*,
};

static mut COMPARE_BROWSER_VERSIONS: Option<
    unsafe extern "system" fn(PCWSTR, PCWSTR, *mut i32) -> HRESULT,
> = None;
static mut CREATE_ENVIRONMENT: Option<
    unsafe extern "system" fn(PCWSTR, PCWSTR, *mut c_void, *mut c_void) -> HRESULT,
> = None;
static mut GET_VERSION_STRING: Option<unsafe extern "system" fn(PCWSTR, *mut PWSTR) -> HRESULT> =
    None;

#[no_mangle]
extern "system" fn DllMain(_hmod: HMODULE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        unsafe {
            // Install focus hooks before the offline client or RML can create UI.
            if std::env::var("RML_BACKGROUND_TESTS").as_deref() == Ok("1") {
                if LoadLibraryA(PCSTR(
                    c"RobloxModLoader\\background_tests.dll".as_ptr() as *const u8
                ))
                .is_err()
                {
                    windows::Win32::System::Threading::ExitProcess(86);
                }
            }
            let _ = LoadLibraryA(PCSTR(c"studio_offline.dll".as_ptr() as *const u8));

            let lib = LoadLibraryA(PCSTR(c"WebView2LoaderOld.dll".as_ptr() as *const u8));

            if let Ok(lib) = lib {
                let addr =
                    GetProcAddress(lib, PCSTR(c"CompareBrowserVersions".as_ptr() as *const u8));
                if let Some(addr) = addr {
                    COMPARE_BROWSER_VERSIONS = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(PCWSTR, PCWSTR, *mut i32) -> HRESULT,
                    >(addr));
                }

                let addr = GetProcAddress(
                    lib,
                    PCSTR(c"CreateCoreWebView2EnvironmentWithOptions".as_ptr() as *const u8),
                );
                if let Some(addr) = addr {
                    CREATE_ENVIRONMENT = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(
                            PCWSTR,
                            PCWSTR,
                            *mut c_void,
                            *mut c_void,
                        ) -> HRESULT,
                    >(addr));
                }

                let addr = GetProcAddress(
                    lib,
                    PCSTR(c"GetAvailableCoreWebView2BrowserVersionString".as_ptr() as *const u8),
                );
                if let Some(addr) = addr {
                    GET_VERSION_STRING = Some(std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(PCWSTR, *mut PWSTR) -> HRESULT,
                    >(addr));
                }
            }
        }
    }
    TRUE
}
/// # Safety
/// CLIPPY NEEDS THIS.
#[no_mangle]
pub unsafe extern "system" fn CompareBrowserVersions(
    version1: PCWSTR,
    version2: PCWSTR,
    result: *mut i32,
) -> HRESULT {
    if let Some(func) = COMPARE_BROWSER_VERSIONS {
        func(version1, version2, result)
    } else {
        E_FAIL
    }
}

/// # Safety
/// CLIPPY NEEDS THIS.
#[no_mangle]
pub unsafe extern "system" fn CreateCoreWebView2EnvironmentWithOptions(
    browser_executable_folder: PCWSTR,
    user_data_folder: PCWSTR,
    environment_options: *mut c_void,
    environment_created_handler: *mut c_void,
) -> HRESULT {
    if let Some(func) = CREATE_ENVIRONMENT {
        func(
            browser_executable_folder,
            user_data_folder,
            environment_options,
            environment_created_handler,
        )
    } else {
        E_FAIL
    }
}

/// # Safety
/// CLIPPY NEEDS THIS.
#[no_mangle]
pub unsafe extern "system" fn GetAvailableCoreWebView2BrowserVersionString(
    browser_executable_folder: PCWSTR,
    version_info: *mut PWSTR,
) -> HRESULT {
    if let Some(func) = GET_VERSION_STRING {
        func(browser_executable_folder, version_info)
    } else {
        E_FAIL
    }
}
