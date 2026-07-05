use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use sqlx::MySqlPool;

use crate::repository::event_repository;

pub fn hash_password(plain: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .expect("password hashing should not fail")
        .to_string()
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed_hash)
        .is_ok()
}

pub async fn seed_admin_event_if_empty(
    pool: &MySqlPool,
    admin_password: &str,
    event_name: &str,
) -> Result<(), sqlx::Error> {
    if event_repository::count(pool).await? == 0 {
        let admin_pass_hash = hash_password(admin_password);
        event_repository::insert_initial(pool, event_name, &admin_pass_hash).await?;
    }

    Ok(())
}

pub async fn try_login(pool: &MySqlPool, submitted_password: &str) -> Result<bool, sqlx::Error> {
    let Some(event) = event_repository::find_singleton(pool).await? else {
        return Ok(false);
    };

    Ok(verify_password(submitted_password, &event.admin_pass_hash))
}

#[cfg(test)]
mod tests {
    use super::{hash_password, verify_password};

    #[test]
    fn hashes_and_verifies_passwords() {
        let hash = hash_password("correct horse battery staple");

        assert_ne!(hash, "correct horse battery staple");
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }
}
