use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CURVE_EMERGENCY_AUTHORITY");
    let authority = env::var("CURVE_EMERGENCY_AUTHORITY")
        .expect("Set CURVE_EMERGENCY_AUTHORITY to a nonzero 64-character hex account ID");
    assert_eq!(
        authority.len(),
        64,
        "CURVE_EMERGENCY_AUTHORITY must be exactly 64 hex characters"
    );
    assert!(
        authority.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "CURVE_EMERGENCY_AUTHORITY must contain only hexadecimal digits"
    );
    let bytes: Vec<_> = authority
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(
                std::str::from_utf8(pair).expect("Authority must be ASCII"),
                16,
            )
            .expect("Authority must be hexadecimal")
        })
        .collect();
    assert!(
        bytes.iter().any(|byte| *byte != 0),
        "CURVE_EMERGENCY_AUTHORITY must not be the default account ID"
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    fs::write(
        output.join("emergency_authority.rs"),
        format!("pub const INITIAL_EMERGENCY_AUTHORITY: AccountId = AccountId::new({bytes:?});\n"),
    )
    .expect("Cannot write emergency authority build output");
}
