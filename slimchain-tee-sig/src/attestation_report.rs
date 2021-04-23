use chrono::NaiveDateTime;
use core::convert::TryFrom;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sgx_types::sgx_quote_t;
use slimchain_common::{
    basic::H256,
    digest::{blake2b_hash_to_h256, default_blake2, Digestible},
    error::{anyhow, bail, ensure, Context as _, Error, Result},
};
use webpki::{EndEntityCert, TlsClientTrustAnchors, TrustAnchor};
use x509_parser::parse_x509_certificate;

include!(concat!(env!("OUT_DIR"), "/root_ca.rs"));

const ROOT_CA_NAME: &str =
    "C=US, ST=CA, L=Santa Clara, O=Intel Corporation, CN=Intel SGX Attestation Report Signing CA";

fn remove_root_ca_from_cert_chain(input: &str) -> Vec<Vec<u8>> {
    pem::parse_many(input.as_bytes())
        .into_iter()
        .filter(|cert| {
            parse_x509_certificate(&cert.contents)
                .map(|(_, cert)| cert.tbs_certificate.subject.to_string() != ROOT_CA_NAME)
                .unwrap_or(true)
        })
        .map(|cert| cert.contents)
        .collect()
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttestationReport {
    /// SGX attestation Report signature
    pub sig: Vec<u8>,
    /// Attestation Report Signing Certificate Chain in DER format
    pub cert: Vec<Vec<u8>>,
    /// Attestation Verification Report
    pub report: Vec<u8>,
}

impl Digestible for AttestationReport {
    fn to_digest(&self) -> H256 {
        let mut cert_hash_state = default_blake2().to_state();
        for c in &self.cert {
            cert_hash_state.update(c.to_digest().as_bytes());
        }
        let cert_hash = blake2b_hash_to_h256(cert_hash_state.finalize());

        let mut hash_state = default_blake2().to_state();
        hash_state.update(self.sig.to_digest().as_bytes());
        hash_state.update(cert_hash.as_bytes());
        hash_state.update(self.report.to_digest().as_bytes());
        let hash = hash_state.finalize();
        blake2b_hash_to_h256(hash)
    }
}

impl AttestationReport {
    pub fn new(sig: Vec<u8>, pem_cert: &str, report: Vec<u8>) -> Self {
        Self {
            sig,
            cert: remove_root_ca_from_cert_chain(pem_cert),
            report,
        }
    }

    pub fn verify(&self, msg: &[u8]) -> Result<()> {
        if cfg!(sim_enclave) {
            return Ok(());
        }

        let report: JsonValue = serde_json::from_slice(&self.report)?;
        trace!(
            "Quote report:\n{}",
            serde_json::to_string_pretty(&report).unwrap()
        );

        let report_time_str = report["timestamp"]
            .as_str()
            .context("Failed to get timestamp.")?;
        let report_time = report_time_str.parse::<NaiveDateTime>()?;

        let (end_cert_der, intermediate_certs_der) = self
            .cert
            .as_slice()
            .split_first()
            .context("No cert is found.")?;

        let end_cert = EndEntityCert::try_from(&end_cert_der[..])
            .map_err(|e| anyhow!("Failed to parse cert. Reason: {}", e))?;
        let intermediate_certs: Vec<_> = intermediate_certs_der.iter().map(|c| &c[..]).collect();

        end_cert
            .verify_is_valid_tls_client_cert(
                &[&webpki::RSA_PKCS1_2048_8192_SHA256],
                &TlsClientTrustAnchors(&INTEL_SGX_ROOT_CA),
                &intermediate_certs[..],
                webpki::Time::from_seconds_since_unix_epoch(report_time.timestamp() as u64),
            )
            .map_err(|e| anyhow!("Failed to verify cert. Reason: {}", e))?;

        end_cert
            .verify_signature(&webpki::RSA_PKCS1_2048_8192_SHA256, &self.report, &self.sig)
            .map_err(|e| anyhow!("Failed to verify sig. Reason: {}", e))?;

        let quote_status = report["isvEnclaveQuoteStatus"]
            .as_str()
            .context("Failed to get isvEnclaveQuoteStatus.")?;

        match quote_status {
            "OK" => {}
            "GROUP_OUT_OF_DATE"
            | "CONFIGURATION_NEEDED"
            | "SW_HARDENING_NEEDED"
            | "CONFIGURATION_AND_SW_HARDENING_NEEDED" => {
                debug!("quote status is {}", quote_status);
            }
            status => {
                bail!("Invalid quote status {}.", status);
            }
        }

        let encoded_quote = report["isvEnclaveQuoteBody"]
            .as_str()
            .context("Failed to get isvEnclaveQuoteBody.")?;
        let quote_data = base64::decode(encoded_quote.as_bytes()).map_err(Error::msg)?;
        let quote: sgx_quote_t = unsafe { std::ptr::read(quote_data.as_ptr() as *const _) };

        // TODO verify measurement w.r.t. the enclave code

        let report_data = quote.report_body.report_data;
        ensure!(
            msg == &report_data.d[..msg.len()],
            "Invalid message in AttestationReport."
        );

        Ok(())
    }
}
