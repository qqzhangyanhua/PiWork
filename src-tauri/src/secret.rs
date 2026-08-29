#[cfg(any(windows, target_os = "macos"))]
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

#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "CoDo";
// load/delete take only the target, so Keychain items share this account.

#[cfg(target_os = "macos")]
pub fn store(target: &str, _username: &str, value: &str) -> Result<(), String> {
    use security_framework::passwords::set_generic_password;

    let mut secret = value.as_bytes().to_vec();
    let result = set_generic_password(target, KEYCHAIN_ACCOUNT, &secret)
        .map_err(|_| "macOS Keychain rejected the credential".to_string());
    secret.zeroize();
    result
}

#[cfg(target_os = "macos")]
pub fn load(target: &str) -> Result<String, String> {
    use security_framework::passwords::{PasswordOptions, generic_password};

    let bytes = generic_password(PasswordOptions::new_generic_password(
        target,
        KEYCHAIN_ACCOUNT,
    ))
    .map_err(|_| "macOS Keychain does not contain the credential".to_string())?;
    String::from_utf8(bytes).map_err(|_| "stored credential is invalid".into())
}

#[cfg(target_os = "macos")]
pub fn delete(target: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password;

    delete_generic_password(target, KEYCHAIN_ACCOUNT)
        .map_err(|_| "macOS Keychain could not remove the credential".into())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn store(_target: &str, _username: &str, _value: &str) -> Result<(), String> {
    Err("secure credential storage is unavailable on this platform".into())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn load(_target: &str) -> Result<String, String> {
    Err("secure credential storage is unavailable on this platform".into())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn delete(_target: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    struct Cleanup(String);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = super::delete(&self.0);
        }
    }

    #[test]
    fn stored_secret_can_be_loaded_and_removed() {
        let target = format!("PiWork/test/{}", uuid::Uuid::new_v4());
        let _cleanup = Cleanup(target.clone());

        super::store(&target, "PiWork", "sk-test-keychain-roundtrip")
            .expect("macOS Keychain must accept a new credential");
        assert_eq!(
            super::load(&target).expect("stored credential must be readable"),
            "sk-test-keychain-roundtrip"
        );

        super::delete(&target).expect("stored credential must be removable");
        assert!(
            super::load(&target).is_err(),
            "deleted credential must not load"
        );
    }

    #[test]
    fn storing_again_replaces_the_secret() {
        let target = format!("PiWork/test/{}", uuid::Uuid::new_v4());
        let _cleanup = Cleanup(target.clone());

        super::store(&target, "PiWork", "first-secret").unwrap();
        super::store(&target, "PiWork", "second-secret").unwrap();
        assert_eq!(super::load(&target).unwrap(), "second-secret");
    }
}
