//! Security tests: authentication, JWT handling, role checks.

use toroidal_db::auth::AuthService;

fn service() -> AuthService {
    AuthService::new("test-secret-for-unit-tests".to_string())
}

#[test]
fn register_and_authenticate_user() {
    let auth = service();
    auth.register_user(
        "alice".to_string(),
        "S3cure-Pass!".to_string(),
        vec!["admin".to_string()],
    )
    .unwrap();

    let token = auth
        .authenticate("alice", "S3cure-Pass!")
        .expect("correct password must authenticate");
    let claims = auth.validate_token(&token.access_token).unwrap();
    assert_eq!(claims.sub, "alice");
}

#[test]
fn wrong_password_is_rejected() {
    let auth = service();
    auth.register_user(
        "bob".to_string(),
        "hunter2-is-bad".to_string(),
        vec!["user".to_string()],
    )
    .unwrap();
    assert!(auth.authenticate("bob", "wrong-password").is_err());
}

#[test]
fn unknown_user_is_rejected() {
    let auth = service();
    assert!(auth.authenticate("ghost", "whatever").is_err());
}

#[test]
fn tampered_token_is_invalid() {
    let auth = service();
    auth.register_user(
        "carol".to_string(),
        "an0ther-pass".to_string(),
        vec!["user".to_string()],
    )
    .unwrap();
    let token = auth.authenticate("carol", "an0ther-pass").unwrap();
    let tampered = format!("{}.x", token.access_token);
    assert!(auth.validate_token(&tampered).is_err());
}

#[test]
fn roles_are_enforced_on_claims() {
    let auth = service();
    auth.register_user(
        "dave".to_string(),
        "role-check-pass".to_string(),
        vec!["admin".to_string()],
    )
    .unwrap();
    let token = auth.authenticate("dave", "role-check-pass").unwrap();
    let claims = auth.validate_token(&token.access_token).unwrap();
    assert!(auth.has_role(&claims, "admin"));
    assert!(!auth.has_role(&claims, "superadmin"));
}
