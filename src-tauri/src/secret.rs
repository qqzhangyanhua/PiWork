use zeroize::Zeroize;

#[cfg(windows)]
pub fn store(target: &str, username: &str, value: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
    };

    let mut target = wide(target);
    let mut username = wide(username);
    let mut secret = value.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        CredentialBlobSize: secret
            .len()
            .try_into()
            .map_err(|_| "credential is too long")?,
        CredentialBlob: secret.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: username.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: Every pointer references a live, NUL-terminated buffer for this call.
    let result = unsafe { CredWriteW(&credential, 0) };
    secret.zeroize();
    if result == 0 {
        return Err("Windows Credential Manager rejected the credential".into());
    }
    Ok(())
}

#[cfg(windows)]
pub fn load(target: &str) -> Result<String, String> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };

    let target = wide(target);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: target is NUL-terminated and credential is a valid out pointer.
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        return Err("Windows Credential Manager does not contain the credential".into());
    }
    // SAFETY: CredReadW returned a valid allocation until CredFree is called.
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        )
        .to_vec()
    };
    // SAFETY: credential was allocated by CredReadW.
    unsafe { CredFree(credential.cast()) };
    String::from_utf8(bytes).map_err(|_| "stored credential is invalid".into())
}

#[cfg(windows)]
pub fn delete(target: &str) -> Result<(), String> {
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};

    let target = wide(target);
    // Missing credentials are treated as an idempotent delete by callers only
    // after they have attempted to load the value.
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        return Err("Windows Credential Manager could not remove the credential".into());
    }
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
pub fn store(_target: &str, _username: &str, _value: &str) -> Result<(), String> {
    Err("secure credential storage is unavailable on this platform".into())
}

#[cfg(not(windows))]
pub fn load(_target: &str) -> Result<String, String> {
    Err("secure credential storage is unavailable on this platform".into())
}

#[cfg(not(windows))]
pub fn delete(_target: &str) -> Result<(), String> {
    Ok(())
}
