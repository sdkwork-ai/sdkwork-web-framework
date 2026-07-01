/// FNV-1a 64-bit — stable across Rust versions and processes (unlike `DefaultHasher`).
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x00000100000001B3;

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Stable short hash for rate-limit / idempotency keys (avoids logging raw tenant ids in keys).
pub fn hash_key_material(material: &str) -> String {
    format!("{:016x}", fnv1a64(material.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_across_calls() {
        let a = hash_key_material("tenant-1:path:/app/v3/api/orders");
        let b = hash_key_material("tenant-1:path:/app/v3/api/orders");
        assert_eq!(a, b);
        assert_ne!(a, hash_key_material("tenant-2:path:/app/v3/api/orders"));
    }
}
