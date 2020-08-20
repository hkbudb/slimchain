// Ref: https://api.trustedservices.intel.com/documents/sgx-attestation-api-spec.pdf

use percent_encoding::percent_decode_str;
use sgx_types::*;
use slimchain_common::error::{ensure, Context as _, Error, Result};
use slimchain_tee_sig::AttestationReport;
use std::io::Read;

const BASE_URL: &str = "https://api.trustedservices.intel.com/sgx/dev";

pub(crate) fn get_intel_sigrl(quote_gid: sgx_epid_group_id_t, api_key: &str) -> Result<Vec<u8>> {
    if cfg!(sim_enclave) {
        return Ok(Vec::new());
    }

    let gid = (quote_gid[0] as u32)
        | ((quote_gid[1] as u32) << 8)
        | ((quote_gid[2] as u32) << 16)
        | ((quote_gid[3] as u32) << 24);
    let gid_hex = hex::encode(gid.to_be_bytes());
    let resp = ureq::get(format!("{}/attestation/v4/sigrl/{}", BASE_URL, gid_hex).as_str())
        .set("Ocp-Apim-Subscription-Key", api_key)
        .call();

    ensure!(
        resp.ok(),
        "Failed to make http request. Status code: {}.",
        resp.status()
    );
    let mut reader = resp.into_reader();
    let mut body = Vec::new();
    reader.read_to_end(&mut body)?;
    base64::decode(body).map_err(Error::msg)
}

pub(crate) fn get_intel_report(quote: &[u8], api_key: &str) -> Result<AttestationReport> {
    let encoded_quote = base64::encode(quote);
    let encoded_body = ureq::json!({ "isvEnclaveQuote": encoded_quote });

    let resp = ureq::post(format!("{}/attestation/v4/report", BASE_URL).as_str())
        .set("Ocp-Apim-Subscription-Key", &api_key)
        .send_json(encoded_body);

    ensure!(
        resp.ok(),
        "Failed to make http request. Status code: {}.",
        resp.status()
    );

    let encoded_sig = resp
        .header("X-IASReport-Signature")
        .context("Failed to retrieve sig.")?;
    let sig = base64::decode(&encoded_sig).map_err(Error::msg)?;
    let encoded_cert = resp
        .header("X-IASReport-Signing-Certificate")
        .context("Failed to retrieve cert.")?;
    let cert = percent_decode_str(encoded_cert).decode_utf8()?.to_string();
    let mut reader = resp.into_reader();
    let mut body = Vec::new();
    reader.read_to_end(&mut body)?;

    Ok(AttestationReport::new(sig, &cert, body))
}
