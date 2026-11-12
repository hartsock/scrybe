// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Shawn Hartsock and contributors

//! PKCE (RFC 7636) verifier/challenge pair and the CSRF `state` nonce.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::Rng;
use sha2::{Digest, Sha256};

/// RFC 7636 §4.1 allows 43–128 unreserved characters; 64 is plenty.
const VERIFIER_LEN: usize = 64;
const STATE_LEN: usize = 32;
const UNRESERVED: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// A code verifier and its S256 challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// A fresh random verifier with its challenge.
    pub fn generate() -> Self {
        Self::from_verifier(&random_unreserved(VERIFIER_LEN))
    }

    /// `challenge = base64url_nopad(sha256(verifier))`.
    pub fn from_verifier(verifier: &str) -> Self {
        let digest = Sha256::digest(verifier.as_bytes());
        Self {
            verifier: verifier.to_string(),
            challenge: URL_SAFE_NO_PAD.encode(digest),
        }
    }
}

/// An unguessable `state` value for the authorization request.
pub fn random_state() -> String {
    random_unreserved(STATE_LEN)
}

fn random_unreserved(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| char::from(UNRESERVED[rng.gen_range(0..UNRESERVED.len())]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_appendix_b_vector() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generated_verifier_is_valid_and_unique() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert!((43..=128).contains(&a.verifier.len()));
        assert!(a.verifier.bytes().all(|c| UNRESERVED.contains(&c)));
        assert_ne!(a.verifier, b.verifier);
        assert_eq!(a, Pkce::from_verifier(&a.verifier));
    }

    #[test]
    fn state_is_random_and_url_safe() {
        let a = random_state();
        let b = random_state();
        assert_eq!(a.len(), STATE_LEN);
        assert!(a.bytes().all(|c| UNRESERVED.contains(&c)));
        assert_ne!(a, b);
    }
}
