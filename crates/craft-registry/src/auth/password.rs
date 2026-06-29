//! Password hashing utilities using Argon2

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use crate::error::{RegistryError, RegistryResult};

/// Hash a password using Argon2
pub fn hash_password(password: &str) -> RegistryResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| RegistryError::Auth(format!("Failed to hash password: {}", e)))?;

    Ok(password_hash.to_string())
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> RegistryResult<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| RegistryError::Auth(format!("Invalid password hash: {}", e)))?;

    let argon2 = Argon2::default();

    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let password = "my_secure_password_123";
        let hash = hash_password(password).unwrap();

        // Verify correct password
        assert!(verify_password(password, &hash).unwrap());

        // Verify incorrect password fails
        assert!(!verify_password("wrong_password", &hash).unwrap());
    }

    #[test]
    fn test_different_passwords_different_hashes() {
        let password1 = "password123";
        let password2 = "password124";

        let hash1 = hash_password(password1).unwrap();
        let hash2 = hash_password(password2).unwrap();

        // Same password should verify against its hash
        assert!(verify_password(password1, &hash1).unwrap());
        assert!(verify_password(password2, &hash2).unwrap());

        // Different passwords should not verify
        assert!(!verify_password(password1, &hash2).unwrap());
        assert!(!verify_password(password2, &hash1).unwrap());
    }
}
