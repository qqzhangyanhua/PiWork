use super::CredentialVault;

pub struct PlatformCredentialVault;

impl PlatformCredentialVault {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
impl CredentialVault for PlatformCredentialVault {
    fn store_api_key(&self, configuration_id: &str, api_key: &str) -> Result<(), String> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
        };

        let mut target = wide(&credential_target(configuration_id));
        let mut username = wide("PiWork");
        let mut secret = api_key.as_bytes().to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: secret.len().try_into().map_err(|_| "API key is too long")?,
            CredentialBlob: secret.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: username.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: All pointers reference live buffers for the duration of the call.
        let result = unsafe { CredWriteW(&credential, 0) };
        secret.fill(0);
        if result == 0 {
            return Err("Windows Credential Manager rejected the API key".into());
        }
        Ok(())
    }

    fn delete_api_key(&self, configuration_id: &str) -> Result<(), String> {
        use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
        let target = wide(&credential_target(configuration_id));
        // SAFETY: target is NUL-terminated and alive for the duration of the call.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            return Err("Windows Credential Manager could not remove the API key".into());
        }
        Ok(())
    }

    fn load_api_key(&self, configuration_id: &str) -> Result<String, String> {
        use windows_sys::Win32::Security::Credentials::{
            CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
        };
        let target = wide(&credential_target(configuration_id));
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: target is NUL-terminated and credential is a valid out pointer.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
            return Err("Windows Credential Manager does not contain the API key".into());
        }
        // SAFETY: CredReadW returned a valid credential allocation until CredFree is called.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (*credential).CredentialBlob,
                (*credential).CredentialBlobSize as usize,
            )
            .to_vec()
        };
        // SAFETY: credential was allocated by CredReadW.
        unsafe { CredFree(credential.cast()) };
        String::from_utf8(bytes).map_err(|_| "stored API key is invalid".into())
    }

    fn load_legacy_api_key(&self) -> Result<String, String> {
        load_windows_credential("PiWork/default-model-api-key")
    }

    fn delete_legacy_api_key(&self) -> Result<(), String> {
        delete_windows_credential("PiWork/default-model-api-key")
    }
}

#[cfg(windows)]
fn credential_target(configuration_id: &str) -> String {
    format!("PiWork/model-api-key/{configuration_id}")
}

#[cfg(windows)]
fn load_windows_credential(target: &str) -> Result<String, String> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };
    let target = wide(target);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        return Err("Windows Credential Manager does not contain the API key".into());
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        )
        .to_vec()
    };
    unsafe { CredFree(credential.cast()) };
    String::from_utf8(bytes).map_err(|_| "stored API key is invalid".into())
}

#[cfg(windows)]
fn delete_windows_credential(target: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
    let target = wide(target);
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        return Err("Windows Credential Manager could not remove the API key".into());
    }
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
impl CredentialVault for PlatformCredentialVault {
    fn store_api_key(&self, _configuration_id: &str, _api_key: &str) -> Result<(), String> {
        Err("secure credential storage is unavailable on this platform".into())
    }

    fn delete_api_key(&self, _configuration_id: &str) -> Result<(), String> {
        Ok(())
    }

    fn load_api_key(&self, _configuration_id: &str) -> Result<String, String> {
        Err("secure credential storage is unavailable on this platform".into())
    }

    fn load_legacy_api_key(&self) -> Result<String, String> {
        Err("secure credential storage is unavailable on this platform".into())
    }

    fn delete_legacy_api_key(&self) -> Result<(), String> {
        Ok(())
    }
}
