//! Release-only signature verification. No network, source sessions or private key access.
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};

#[test]
fn signed_release_artifacts_match_the_embedded_public_key_and_reject_tampering() {
    let Some(directory) = std::env::var_os("TOKLENS_RELEASE_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).expect("configuration");
    let key = STANDARD
        .decode(
            config["plugins"]["updater"]["pubkey"]
                .as_str()
                .expect("public key"),
        )
        .expect("key encoding");
    let key = PublicKey::decode(std::str::from_utf8(&key).expect("key text")).expect("public key");
    let mut verified = 0;
    for file in std::fs::read_dir(directory).expect("release artifacts") {
        let path = file.expect("entry").path();
        if path.extension().is_none_or(|extension| extension != "sig") {
            continue;
        }
        let signed_path = path.with_extension("");
        let mut bytes = std::fs::read(signed_path).expect("package");
        let signature = std::fs::read_to_string(path).expect("signature");
        let decoded = STANDARD
            .decode(signature.trim())
            .expect("signature encoding");
        let signature = Signature::decode(std::str::from_utf8(&decoded).expect("signature text"))
            .expect("signature");
        key.verify(&bytes, &signature, true)
            .expect("release signature must match embedded key");
        let signed_version = signature
            .trusted_comment()
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"));
        assert_eq!(
            signed_version,
            config["version"].as_str(),
            "signed version must match the release manifest"
        );
        bytes.push(0);
        assert!(
            key.verify(&bytes, &signature, true).is_err(),
            "tampered packages must be rejected"
        );
        verified += 1;
    }
    assert_eq!(
        verified, 1,
        "each platform must provide one signed updater package"
    );
}
