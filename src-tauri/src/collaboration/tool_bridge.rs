//! Run-scoped Host Tool token and lease registry.
//!
//! A Lease binds exactly one Run to a token whose SHA-256 digest is the only
//! stored form; the registry never holds plaintext. Authentication compares the
//! presented token digest in constant time, and the token zeroizes on drop.
//! The loopback HTTP transport that consumes these leases is layered on top so
//! the security core stays independently testable.

use std::{
    collections::HashMap,
    fmt,
    sync::Mutex,
};

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const TOKEN_BYTES: usize = 32;

/// A 32-byte CSPRNG token that zeroizes on drop and redacts in Debug output.
pub struct HostToolToken([u8; TOKEN_BYTES]);

impl HostToolToken {
    /// Generates a token from the OS CSPRNG via two v4 UUIDs (16 bytes each).
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_BYTES];
        bytes[..16].copy_from_slice(&uuid::Uuid::new_v4().into_bytes());
        bytes[16..].copy_from_slice(&uuid::Uuid::new_v4().into_bytes());
        Self(bytes)
    }

    fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.0).into()
    }

    /// The raw bytes to inject into the Run environment for the extension to
    /// present back over the loopback transport.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Hex encoding for injection into the Run environment and loopback JSON.
    pub fn to_hex(&self) -> String {
        let mut hex = String::with_capacity(TOKEN_BYTES * 2);
        for byte in self.0 {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }
}

impl Drop for HostToolToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for HostToolToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HostToolToken([REDACTED])")
    }
}

/// The context that authorizes one Run's host-tool lease.
#[derive(Debug, Clone)]
pub struct AuthorizedRunContext {
    pub run_id: String,
    pub work_id: String,
    pub assignment_id: String,
    pub agent_instance_id: String,
    pub runtime_owner: String,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug)]
pub struct HostToolLease {
    pub endpoint: String,
    pub token: HostToolToken,
    pub allowed_tools: Vec<String>,
}

/// Stores token digests and the authorizing Run context keyed by Run id;
/// `authenticate` compares digests in constant time.
#[derive(Default)]
pub struct HostToolRegistry {
    leases: Mutex<HashMap<String, (Vec<u8>, AuthorizedRunContext)>>,
}

impl HostToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issue(&self, context: AuthorizedRunContext, endpoint: String) -> HostToolLease {
        let token = HostToolToken::generate();
        self.leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                context.run_id.clone(),
                (token.digest().to_vec(), context.clone()),
            );
        HostToolLease {
            endpoint,
            token,
            allowed_tools: context.allowed_tools,
        }
    }

    /// Constant-time authentication of a presented token against the lease.
    pub fn authenticate(&self, run_id: &str, presented: &[u8]) -> bool {
        let expected = self
            .leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(run_id)
            .map(|(digest, _context)| digest.clone());
        let Some(expected) = expected else {
            return false;
        };
        let digest: [u8; 32] = Sha256::digest(presented).into();
        constant_time_eq(&digest, &expected)
    }

    /// The authorizing Run context for a dispatched tool call.
    pub fn context(&self, run_id: &str) -> Option<AuthorizedRunContext> {
        self.leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(run_id)
            .map(|(_digest, context)| context.clone())
    }

    pub fn revoke(&self, run_id: &str) {
        self.leases
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(run_id);
    }

    #[cfg(test)]
    fn stores_plaintext(&self, token: &HostToolToken) -> bool {
        let leases = self.leases.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        leases
            .values()
            .any(|(digest, _context)| digest.as_slice() == token.0.as_slice())
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(run_id: &str) -> AuthorizedRunContext {
        AuthorizedRunContext {
            run_id: run_id.to_owned(),
            work_id: "work-1".to_owned(),
            assignment_id: "assignment-1".to_owned(),
            agent_instance_id: "agent-1".to_owned(),
            runtime_owner: "owner-1".to_owned(),
            allowed_tools: vec!["delegate_assignment".to_owned()],
        }
    }

    #[test]
    fn lease_token_authenticates_constant_time_and_revokes() {
        let registry = HostToolRegistry::new();
        let lease = registry.issue(context("run-1"), "http://127.0.0.1:0/tool".to_owned());

        let token = lease.token.as_bytes();
        assert!(registry.authenticate("run-1", token));
        assert!(!registry.authenticate("run-1", &[0u8; 32]));
        assert!(!registry.authenticate("run-2", token));

        registry.revoke("run-1");
        assert!(!registry.authenticate("run-1", token));
    }

    #[test]
    fn registry_never_stores_plaintext() {
        let registry = HostToolRegistry::new();
        let lease = registry.issue(context("run-1"), "http://127.0.0.1:0/tool".to_owned());
        assert!(!registry.stores_plaintext(&lease.token));
    }

    #[test]
    fn token_debug_is_redacted() {
        let token = HostToolToken::generate();
        let debug = format!("{token:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains("HostToolToken([0, 1"));
    }
}
