use base64::{engine::general_purpose::STANDARD, Engine as _};

const DPAPI_PREFIX: &str = "dpapi:";

pub fn is_protected(value: &str) -> bool {
    value.starts_with(DPAPI_PREFIX)
}

pub fn protect_string(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(value.to_string());
    }
    protect_bytes(value.as_bytes()).map(|bytes| format!("{DPAPI_PREFIX}{}", STANDARD.encode(bytes)))
}

pub fn unprotect_string(value: &str) -> Result<String, String> {
    if !is_protected(value) {
        return Ok(value.to_string());
    }
    let encoded = value
        .strip_prefix(DPAPI_PREFIX)
        .ok_or_else(|| "凭据格式无效".to_string())?;
    if encoded.trim().is_empty() {
        return Err("凭据编码为空".to_string());
    }
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "凭据编码无效".to_string())?;
    let plain = unprotect_bytes(&encrypted)?;
    String::from_utf8(plain).map_err(|_| "凭据不是有效 UTF-8".to_string())
}

#[cfg(windows)]
fn protect_bytes(value: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::null;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: All pointers are valid for the duration of the call, UI is disabled, and output is
    // released with LocalFree immediately after copying.
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows DPAPI 加密失败".to_string());
    }
    if output.pbData.is_null() {
        return Err("Windows DPAPI 加密结果为空".to_string());
    }

    // SAFETY: On success, DPAPI returns `cbData` bytes at `pbData`. The slice is copied before
    // freeing the buffer.
    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    // SAFETY: `pbData` was allocated by DPAPI and must be released with LocalFree once.
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

#[cfg(windows)]
fn unprotect_bytes(value: &[u8]) -> Result<Vec<u8>, String> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: value.len() as u32,
        pbData: value.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: All pointers are valid for the duration of the call, UI is disabled, and output is
    // released with LocalFree immediately after copying.
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            null(),
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows DPAPI 解密失败".to_string());
    }
    if output.pbData.is_null() {
        return Err("Windows DPAPI 解密结果为空".to_string());
    }

    // SAFETY: On success, DPAPI returns `cbData` bytes at `pbData`. The slice is copied before
    // freeing the buffer.
    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    // SAFETY: `pbData` was allocated by DPAPI and must be released with LocalFree once.
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(plain)
}

#[cfg(not(windows))]
fn protect_bytes(_value: &[u8]) -> Result<Vec<u8>, String> {
    Err("当前平台不支持 DPAPI 凭据加密".to_string())
}

#[cfg(not(windows))]
fn unprotect_bytes(_value: &[u8]) -> Result<Vec<u8>, String> {
    Err("当前平台不支持 DPAPI 凭据解密".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_plain_legacy_value_readable() {
        assert_eq!(unprotect_string("legacy-token").unwrap(), "legacy-token");
    }

    #[test]
    fn rejects_empty_protected_payload() {
        assert_eq!(unprotect_string("dpapi:").unwrap_err(), "凭据编码为空");
        assert_eq!(unprotect_string("dpapi:   ").unwrap_err(), "凭据编码为空");
    }

    #[cfg(windows)]
    #[test]
    fn protects_and_restores_secret() {
        let protected = protect_string("secret-token").expect("DPAPI 应能加密");
        assert!(is_protected(&protected));
        assert!(!protected.contains("secret-token"));
        assert_eq!(unprotect_string(&protected).unwrap(), "secret-token");
    }
}
