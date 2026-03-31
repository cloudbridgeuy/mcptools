use sha2::{Digest, Sha256};

use super::types::ContentHash;

/// Compute the SHA-256 content hash of the given bytes.
///
/// This is a pure function: bytes in, `ContentHash` out.
pub fn content_hash(bytes: &[u8]) -> ContentHash {
    let digest = Sha256::digest(bytes);
    ContentHash::new(digest.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_input_produces_known_hash() {
        // SHA-256 of "hello" is well-known:
        // 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        let hash = content_hash(b"hello");
        assert_eq!(
            hash.hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn empty_input_produces_correct_hash() {
        // SHA-256 of "" is:
        // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = content_hash(b"");
        assert_eq!(
            hash.hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn different_inputs_produce_different_hashes() {
        let hash_a = content_hash(b"hello");
        let hash_b = content_hash(b"world");
        assert_ne!(hash_a, hash_b);
    }
}
