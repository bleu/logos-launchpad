use std::{env, fs, path::PathBuf};

const ROLE_AUTHORITY_SETTING: &str = "LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT";
const DEFAULT_ROLE_AUTHORITY: [u8; 32] = [
    94, 144, 155, 106, 14, 146, 62, 120, 135, 16, 55, 144, 76, 89, 251, 81, 15, 154, 235, 51, 72,
    22, 124, 218, 6, 52, 198, 24, 253, 158, 192, 243,
];

fn parse_account_id(setting: &str, value: &str) -> [u8; 32] {
    assert_eq!(
        value.len(),
        64,
        "{setting} must be exactly 64 hex characters"
    );
    assert!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{setting} must contain only hexadecimal digits"
    );
    let mut bytes = [0; 32];
    for (byte, pair) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    assert!(
        bytes.iter().any(|byte| *byte != 0),
        "{setting} must not be the default account ID"
    );
    bytes
}

fn main() {
    println!("cargo:rerun-if-env-changed=CURVE_EMERGENCY_AUTHORITY");
    println!("cargo:rerun-if-env-changed={ROLE_AUTHORITY_SETTING}");
    let emergency = env::var("CURVE_EMERGENCY_AUTHORITY")
        .expect("Set CURVE_EMERGENCY_AUTHORITY to a nonzero 64-character hex account ID");
    let emergency = parse_account_id("CURVE_EMERGENCY_AUTHORITY", &emergency);
    let role_authority = match env::var(ROLE_AUTHORITY_SETTING) {
        Ok(address) => parse_account_id(ROLE_AUTHORITY_SETTING, &address),
        Err(env::VarError::NotPresent) => DEFAULT_ROLE_AUTHORITY,
        Err(env::VarError::NotUnicode(_)) => panic!("{ROLE_AUTHORITY_SETTING} must be ASCII hex"),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    fs::write(
        output.join("authority_accounts.rs"),
        format!(
            "pub const INITIAL_EMERGENCY_AUTHORITY: AccountId = AccountId::new({emergency:?});\n\
             pub const AUTHORITY_PROGRAM_ACCOUNT_ID: AccountId = AccountId::new({role_authority:?});\n"
        ),
    )
    .expect("Cannot write authority account build output");
}
