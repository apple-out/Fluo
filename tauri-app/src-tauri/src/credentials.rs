use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Secrets;

const FILE_NAME: &str = "credentials.bin";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredSecrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bridge_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    codex_email: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

pub fn load(dir: &Path) -> Secrets {
    let path = dir.join(FILE_NAME);
    let Ok(encrypted) = fs::read(path) else {
        return Secrets::default();
    };
    let Ok(plain) = unprotect(&encrypted) else {
        return Secrets::default();
    };
    let Ok(stored) = serde_json::from_slice::<StoredSecrets>(&plain) else {
        return Secrets::default();
    };
    Secrets {
        api_key: non_empty(stored.api_key),
        bridge_token: non_empty(stored.bridge_token),
        codex_access_token: non_empty(stored.codex_access_token),
        codex_refresh_token: non_empty(stored.codex_refresh_token),
        codex_account_id: non_empty(stored.codex_account_id),
        codex_expires_at: stored.codex_expires_at,
        codex_email: non_empty(stored.codex_email),
    }
}

pub fn save(dir: &Path, secrets: &Secrets) -> Result<(), String> {
    let stored = StoredSecrets {
        api_key: non_empty(secrets.api_key.clone()),
        bridge_token: non_empty(secrets.bridge_token.clone()),
        codex_access_token: non_empty(secrets.codex_access_token.clone()),
        codex_refresh_token: non_empty(secrets.codex_refresh_token.clone()),
        codex_account_id: non_empty(secrets.codex_account_id.clone()),
        codex_expires_at: secrets.codex_expires_at,
        codex_email: non_empty(secrets.codex_email.clone()),
    };
    let plain = serde_json::to_vec(&stored).map_err(|error| error.to_string())?;
    let encrypted = protect(&plain)?;
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let target = dir.join(FILE_NAME);
    let temporary = dir.join(format!("{FILE_NAME}.tmp"));
    fs::write(&temporary, encrypted).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn encrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    protect(data)
}

pub fn decrypt_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    unprotect(data)
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData};
    use windows::core::PCWSTR;

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&input, PCWSTR::null(), None, None, None, 0, &mut output)
            .map_err(|error| error.message())?;
    }
    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
    }
    Ok(protected)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|error| error.message())?;
    }
    let plain =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(Some(HLOCAL(output.pbData as *mut _)));
    }
    Ok(plain)
}

#[cfg(not(windows))]
fn protect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("secure credential storage is only implemented on Windows".into())
}

#[cfg(not(windows))]
fn unprotect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("secure credential storage is only implemented on Windows".into())
}
