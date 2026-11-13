use crate::config::{get_flick_dir, FlickConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

pub fn get_key_enc_path() -> PathBuf {
    get_flick_dir().join("pc_key.enc")
}

pub fn get_cert_pem_path() -> PathBuf {
    get_flick_dir().join("pc_cert.pem")
}

/// Decrypts DPAPI-encrypted private key from disk or returns Err.
pub fn dpapi_unprotect(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: ciphertext.len() as u32,
            pbData: ciphertext.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let ret = CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut out_blob,
        );

        if ret == 0 {
            return Err("CryptUnprotectData failed".to_string());
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let decrypted = slice.to_vec();
        LocalFree(out_blob.pbData as HLOCAL);
        Ok(decrypted)
    }
}

/// Encrypts bytes using Windows DPAPI (tied to current Windows User SID).
pub fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };
        let mut out_blob = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        let desc_u16: Vec<u16> = "Flick PC ECDSA P-256 Private Key\0"
            .encode_utf16()
            .collect();

        let ret = CryptProtectData(
            &in_blob,
            desc_u16.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut out_blob,
        );

        if ret == 0 {
            return Err("CryptProtectData failed".to_string());
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let encrypted = slice.to_vec();
        LocalFree(out_blob.pbData as HLOCAL);
        Ok(encrypted)
    }
}

/// Calculates 64-char lowercase hex SHA-256 digest of SubjectPublicKeyInfo (SPKI)
pub fn calculate_spki_sha256(der_bytes: &[u8]) -> Result<String, String> {
    let (_, cert) = x509_parser::parse_x509_certificate(der_bytes)
        .map_err(|e| format!("Failed to parse X.509 DER cert: {:?}", e))?;
    let spki_bytes = cert.tbs_certificate.subject_pki.raw;
    let mut hasher = Sha256::new();
    hasher.update(spki_bytes);
    Ok(hex::encode(hasher.finalize()).to_lowercase())
}

/// Calculates 64-char lowercase hex SHA-256 digest of full DER certificate
pub fn calculate_cert_sha256(der_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(der_bytes);
    hex::encode(hasher.finalize()).to_lowercase()
}

#[allow(dead_code)]
pub struct PcCredentials {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub spki_pin: String,
    pub cert_pin: String,
}

/// Loads existing ~/.flick/pc_key.enc and ~/.flick/pc_cert.pem or generates a new pair.
pub fn get_or_create_pc_credentials(cfg: &FlickConfig) -> Result<PcCredentials, String> {
    let key_enc_path = get_key_enc_path();
    let cert_pem_path = get_cert_pem_path();

    if key_enc_path.exists() && cert_pem_path.exists() {
        if let Ok(enc_bytes) = fs::read(&key_enc_path) {
            if let Ok(priv_pem_bytes) = dpapi_unprotect(&enc_bytes) {
                if let Ok(cert_pem_str) = fs::read_to_string(&cert_pem_path) {
                    if let Ok(creds) = parse_credentials_from_pem(&cert_pem_str, &priv_pem_bytes) {
                        return Ok(creds);
                    }
                }
            }
        }
    }

    // Generate new credentials
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .map_err(|e| format!("rcgen KeyPair generation failed: {:?}", e))?;
    let priv_pem = key_pair.serialize_pem();

    let mut params = rcgen::CertificateParams::new(vec![cfg.pc_name.clone()])
        .map_err(|e| format!("rcgen CertificateParams failed: {:?}", e))?;
    params.distinguished_name.push(rcgen::DnType::CommonName, cfg.pc_name.clone());

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("rcgen self_signed failed: {:?}", e))?;
    let cert_pem = cert.pem();
    let cert_der = cert.der().to_vec();

    let enc_key = dpapi_protect(priv_pem.as_bytes())?;
    fs::create_dir_all(get_flick_dir()).map_err(|e| e.to_string())?;
    fs::write(&key_enc_path, enc_key).map_err(|e| e.to_string())?;
    fs::write(&cert_pem_path, &cert_pem).map_err(|e| e.to_string())?;

    let spki_pin = calculate_spki_sha256(&cert_der)?;
    let cert_pin = calculate_cert_sha256(&cert_der);
    let key_der = key_pair.serialize_der();

    Ok(PcCredentials {
        cert_der,
        key_der,
        spki_pin,
        cert_pin,
    })
}

fn parse_credentials_from_pem(
    cert_pem_str: &str,
    priv_pem_bytes: &[u8],
) -> Result<PcCredentials, String> {
    // Parse cert PEM to DER via x509-parser
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem_str.as_bytes())
        .map_err(|e| format!("Failed to parse certificate PEM: {:?}", e))?;
    let cert_der = pem.contents;

    // Parse private key PKCS8 PEM to DER
    let priv_pem_str = std::str::from_utf8(priv_pem_bytes)
        .map_err(|e| format!("Invalid UTF-8 in decrypted private key: {:?}", e))?;
    let key_pair = rcgen::KeyPair::from_pem(priv_pem_str)
        .map_err(|e| format!("Failed to parse private key PEM with rcgen: {:?}", e))?;
    let key_der = key_pair.serialize_der();

    let spki_pin = calculate_spki_sha256(&cert_der)?;
    let cert_pin = calculate_cert_sha256(&cert_der);
    let _ = CACHED_SPKI_PIN.set(spki_pin.clone());

    Ok(PcCredentials {
        cert_der,
        key_der,
        spki_pin,
        cert_pin,
    })
}

static CACHED_SPKI_PIN: OnceLock<String> = OnceLock::new();

pub fn get_pc_spki_pin() -> String {
    if let Some(pin) = CACHED_SPKI_PIN.get() {
        return pin.clone();
    }
    let cfg = crate::config::get_config();
    if let Ok(creds) = get_or_create_pc_credentials(&cfg) {
        let pin = creds.spki_pin.clone();
        let _ = CACHED_SPKI_PIN.set(pin.clone());
        return pin;
    }
    String::new()
}

// ----------------------------------------------------------------------------
// rustls mTLS 1.3 ClientCertVerifier
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub struct FlickClientCertVerifier;

impl rustls::server::danger::ClientCertVerifier for FlickClientCertVerifier {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        Ok(rustls::server::danger::ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn create_rustls_server_config(creds: &PcCredentials) -> Result<Arc<rustls::ServerConfig>, String> {
    let verifier = Arc::new(FlickClientCertVerifier);
    let cert_der = CertificateDer::from(creds.cert_der.clone());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(creds.key_der.clone()));

    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| format!("TLS version configuration error: {:?}", e))?
    .with_client_cert_verifier(verifier)
    .with_single_cert(vec![cert_der], key_der)
    .map_err(|e| format!("Failed to set TLS server certificate: {:?}", e))?;

    Ok(Arc::new(config))
}
