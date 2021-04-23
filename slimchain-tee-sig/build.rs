use std::env;
use std::fs;
use std::path::PathBuf;
use webpki::TrustAnchor;

fn generate_code_for_trust_anchors(name: &str, trust_anchors: &[TrustAnchor]) -> String {
    let decl = format!(
        "static {}: [TrustAnchor<'static>; {}] = ",
        name,
        trust_anchors.len()
    );

    let value = str::replace(&format!("{:?};\n", trust_anchors), ": [", ": &[");

    decl + &value
}

fn gen_root_ca_rs() {
    // Obtain cert from https://api.portal.trustedservices.intel.com/EPID-attestation
    let root_cert_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("Intel_SGX_Attestation_RootCA.cer");
    println!(
        "cargo:rerun-if-changed={}",
        root_cert_path.to_string_lossy()
    );

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let root_cert_data = fs::read(root_cert_path).expect("Failed to read SGX root CA cert.");
    let trust_root =
        TrustAnchor::try_from_cert_der(&root_cert_data[..]).expect("Failed to load trust root.");
    let code = generate_code_for_trust_anchors("INTEL_SGX_ROOT_CA", &[trust_root]);
    fs::write(out_dir.join("root_ca.rs"), code).expect("Failed to write root_ca.rs.");
}

fn set_sgx_mode() {
    let sgx_mode = env::var("SGX_MODE").unwrap_or_else(|_| "HW".to_string());
    println!("cargo:rerun-if-env-changed=SGX_MODE");

    if &sgx_mode == "SW" {
        println!("cargo:rustc-cfg=sim_enclave");
    }
}

fn main() {
    gen_root_ca_rs();
    set_sgx_mode();
}
