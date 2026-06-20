use rust_backend_assessment_af2::auth::{generate_otp_code, hash_code, hash_password, verify_password};

#[test]
fn password_hash_roundtrip() {
    let hashed = hash_password("CorrectPass123!").expect("hash should succeed");
    assert!(verify_password("CorrectPass123!", &hashed).unwrap());
    assert!(!verify_password("WrongPass", &hashed).unwrap());
}

#[test]
fn otp_code_is_six_digits() {
    let code = generate_otp_code();
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn code_hash_is_deterministic_and_one_way() {
    let code = "123456";
    let h1 = hash_code(code);
    let h2 = hash_code(code);
    assert_eq!(h1, h2, "same code must hash identically for verification");
    assert_ne!(h1, code, "the stored hash must not equal the plaintext code");
}