use std::ffi::CStr;

pub type FromComponentsFn = extern "C" fn(
    res: *mut u128,
    schema: usize,
    host: usize,
    path: usize,
    query: usize,
    fragment: usize,
) -> *mut u128;
pub type TrustCheckFn = extern "C" fn(str1: *const i8, a2: i8, a3: i8, a4: i8) -> bool;
pub type HttpRequestNotTrustedFn = extern "C" fn(a1: *mut usize, a2: usize) -> *mut i8;

pub static mut ORIGINAL: Option<FromComponentsFn> = None;
pub static mut OG_TC: Option<TrustCheckFn> = None;
pub static mut ORIGINAL_HTTP_NT: Option<HttpRequestNotTrustedFn> = None;

pub extern "C" fn hook_test(
    res: *mut u128,
    schema: usize,
    host: usize,
    path: usize,
    query: usize,
    fragment: usize,
) -> *mut u128 {
    unsafe {
        let Some(original) = ORIGINAL else {
            return res;
        };
        if host == 0 {
            return original(res, schema, host, path, query, fragment);
        }
        let view = &*(host as *const StringView);
        if view.data.is_null() || view.len == 0 {
            return original(res, schema, host, path, query, fragment);
        }
        let bytes = std::slice::from_raw_parts(view.data, view.len);
        let original_host = std::str::from_utf8(bytes).unwrap_or("");
        if !(original_host == "roblox.com" || original_host.ends_with(".roblox.com")) {
            return original(res, schema, host, path, query, fragment);
        }
        let port = std::env::var("STUDIO_OFFLINE_PORT").unwrap_or_else(|_| "80".to_owned());
        let local = format!("localhost:{port}");
        // Use local views, never change shared input strings owned by the caller.
        let new_host = StringView {
            data: local.as_ptr(),
            len: local.len(),
        };
        let new_scheme = StringView {
            data: b"http".as_ptr(),
            len: 4,
        };
        original(
            res,
            &new_scheme as *const _ as usize,
            &new_host as *const _ as usize,
            path,
            query,
            fragment,
        )
    }
}

pub extern "C" fn trustcheck_hook(str1: *const i8, a2: i8, a3: i8, a4: i8) -> bool {
    unsafe {
        let url = CStr::from_ptr(str1).to_string_lossy();
        let replacement = "http://roblox.com\0";

        if url.contains("http://localhost") && a3 == 0 {
            if let Some(orig) = OG_TC {
                return orig(replacement.as_ptr() as *const i8, a2, a3, a4);
            }
        }

        if let Some(orig) = OG_TC {
            return orig(str1, a2, a3, a4);
        }
        false
    }
}

pub extern "C" fn nottrusted_hook(_a1: *mut usize, _a2: usize) -> *mut i8 {
    c"1".as_ptr() as *mut i8
}

#[repr(C)]
pub struct StringView {
    data: *const u8,
    len: usize,
}
pub type FromStringFn =
    extern "C" fn(*mut std::ffi::c_void, *const StringView) -> *mut std::ffi::c_void;
pub static mut ORIGINAL_FROM_STRING: Option<FromStringFn> = None;

pub extern "C" fn parse_url_hook(
    result: *mut std::ffi::c_void,
    view: *const StringView,
) -> *mut std::ffi::c_void {
    unsafe {
        let Some(original) = ORIGINAL_FROM_STRING else {
            return result;
        };
        if view.is_null() || (*view).data.is_null() || (*view).len == 0 {
            return original(result, view);
        }
        let bytes = std::slice::from_raw_parts((*view).data, (*view).len);
        if let Ok(url) = std::str::from_utf8(bytes) {
            if let Some((scheme, rest)) = url.split_once("://") {
                let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
                let host = &rest[..host_end];
                if (scheme == "http" || scheme == "https")
                    && (host == "roblox.com" || host.ends_with(".roblox.com"))
                {
                    let port =
                        std::env::var("STUDIO_OFFLINE_PORT").unwrap_or_else(|_| "80".to_owned());
                    let local = format!("http://localhost:{port}{}", &rest[host_end..]);
                    let replacement = StringView {
                        data: local.as_ptr(),
                        len: local.len(),
                    };
                    return original(result, &replacement);
                }
            }
        }
        original(result, view)
    }
}
