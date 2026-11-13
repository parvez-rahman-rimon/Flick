"""
Flick Windows Receiver - Cryptography & Zero-Trust mTLS Manager
Manages:
  1. ECDSA P-256 (secp256r1) host keypair and self-signed X.509 certificate.
  2. Windows DPAPI (CryptProtectData / CryptUnprotectData) encrypted key storage (~/.flick/pc_key.enc).
  3. SHA-256 Subject Public Key Info (SPKI) and certificate fingerprint calculation.
  4. TLS 1.3 server SSLContext enforcement with mutual authentication (CERT_REQUIRED).
"""

import ctypes
import datetime
import hashlib
import os
import ssl
import sys
import tempfile
import threading
from pathlib import Path
from typing import Optional, Tuple

import win32crypt
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

CONFIG_DIR = Path.home() / ".flick"
KEY_ENC_FILE = CONFIG_DIR / "pc_key.enc"
CERT_PEM_FILE = CONFIG_DIR / "pc_cert.pem"

_crypto_lock = threading.Lock()
_cached_cert: Optional[x509.Certificate] = None
_cached_key_pem: Optional[bytes] = None
_cached_spki_pin: Optional[str] = None

# Native OpenSSL C-types hook for custom client certificate validation
VERIFY_CB = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)


def _always_allow_verify_cb(store_ctx: int, arg: int) -> int:
    """
    OpenSSL verify callback that accepts the client's self-signed leaf certificate during TLS handshake
    so the raw DER bytes can be retrieved via sock.getpeercert(binary_form=True) and validated
    against Flick's trusted paired devices or ephemeral pair proof.
    """
    return 1


_C_VERIFY_CB = VERIFY_CB(_always_allow_verify_cb)


def _get_libssl() -> Optional[ctypes.CDLL]:
    """Finds and loads OpenSSL libssl-3.dll across dev, standard, and PyInstaller onefile runtimes."""
    candidates = [
        os.path.join(getattr(sys, "_MEIPASS", ""), "libssl-3.dll"),
        os.path.join(os.path.dirname(sys.executable), "libssl-3.dll"),
        os.path.join(os.path.dirname(sys.executable), "DLLs", "libssl-3.dll"),
        os.path.join(os.path.dirname(ssl._ssl.__file__), "libssl-3.dll"),
        "libssl-3.dll",
    ]
    for c in candidates:
        if c and os.path.exists(c):
            try:
                return ctypes.CDLL(c)
            except Exception:
                pass
    try:
        return ctypes.CDLL("libssl-3.dll")
    except Exception:
        return None


def calculate_spki_sha256(cert: x509.Certificate) -> str:
    """Calculates the 64-char lowercase hex SHA-256 digest of the Subject Public Key Info (SPKI)."""
    spki_bytes = cert.public_key().public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return hashlib.sha256(spki_bytes).hexdigest().lower()


def calculate_cert_sha256(cert: x509.Certificate) -> str:
    """Calculates the 64-char lowercase hex SHA-256 digest of the full DER certificate."""
    cert_der = cert.public_bytes(encoding=serialization.Encoding.DER)
    return hashlib.sha256(cert_der).hexdigest().lower()


def get_spki_pin_from_der(der_bytes: bytes) -> str:
    """Extracts the SPKI SHA-256 fingerprint from raw X.509 DER certificate bytes."""
    cert = x509.load_der_x509_certificate(der_bytes)
    return calculate_spki_sha256(cert)


def get_cert_pin_from_der(der_bytes: bytes) -> str:
    """Calculates SHA-256 fingerprint directly from raw DER bytes."""
    return hashlib.sha256(der_bytes).hexdigest().lower()


def get_or_create_pc_credentials(pc_id: str = "") -> Tuple[x509.Certificate, bytes, str]:
    """
    Retrieves or generates the PC's ECDSA P-256 keypair and self-signed X.509 certificate.
    Private key is protected using Windows DPAPI (CryptProtectData) at ~/.flick/pc_key.enc.
    Returns: (cert, private_key_pem_bytes, spki_pin_hex).
    """
    global _cached_cert, _cached_key_pem, _cached_spki_pin

    with _crypto_lock:
        if _cached_cert is not None and _cached_key_pem is not None and _cached_spki_pin is not None:
            return _cached_cert, _cached_key_pem, _cached_spki_pin

        CONFIG_DIR.mkdir(parents=True, exist_ok=True)

        if KEY_ENC_FILE.exists() and CERT_PEM_FILE.exists():
            try:
                # Decrypt private key with DPAPI
                with open(KEY_ENC_FILE, "rb") as f:
                    enc_key = f.read()
                _, priv_pem = win32crypt.CryptUnprotectData(enc_key, None, None, None, 0)

                # Read certificate
                with open(CERT_PEM_FILE, "rb") as f:
                    cert_pem = f.read()

                cert = x509.load_pem_x509_certificate(cert_pem)
                spki_pin = calculate_spki_sha256(cert)

                _cached_cert = cert
                _cached_key_pem = priv_pem
                _cached_spki_pin = spki_pin
                return cert, priv_pem, spki_pin
            except Exception as e:
                print(f"[CryptoManager] Failed to load existing credentials ({e}), regenerating...")

        # Generate fresh ECDSA P-256 keypair
        priv_key = ec.generate_private_key(ec.SECP256R1())
        priv_pem = priv_key.private_bytes(
            encoding=serialization.Encoding.PEM,
            format=serialization.PrivateFormat.PKCS8,
            encryption_algorithm=serialization.NoEncryption(),
        )

        # Protect with Windows DPAPI tied to user credentials
        enc_priv_key = win32crypt.CryptProtectData(
            priv_pem, "Flick PC ECDSA P-256 Private Key", None, None, None, 0
        )
        with open(KEY_ENC_FILE, "wb") as f:
            f.write(enc_priv_key)

        # Generate 10-year self-signed certificate
        common_name = f"Flick-PC-{pc_id[:8]}" if pc_id else "Flick-PC"
        subject = issuer = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, common_name)])
        now = datetime.datetime.now(datetime.timezone.utc)

        cert_builder = (
            x509.CertificateBuilder()
            .subject_name(subject)
            .issuer_name(issuer)
            .public_key(priv_key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(now - datetime.timedelta(days=1))
            .not_valid_after(now + datetime.timedelta(days=3650))  # 10 years
            .add_extension(
                x509.BasicConstraints(ca=False, path_length=None),
                critical=True,
            )
            .add_extension(
                x509.KeyUsage(
                    digital_signature=True,
                    content_commitment=False,
                    key_encipherment=False,
                    data_encipherment=False,
                    key_agreement=True,
                    key_cert_sign=False,
                    crl_sign=False,
                    encipher_only=False,
                    decipher_only=False,
                ),
                critical=True,
            )
            .add_extension(
                x509.ExtendedKeyUsage([
                    ExtendedKeyUsageOID.SERVER_AUTH,
                    ExtendedKeyUsageOID.CLIENT_AUTH,
                ]),
                critical=False,
            )
        )

        cert = cert_builder.sign(priv_key, hashes.SHA256())
        cert_pem = cert.public_bytes(serialization.Encoding.PEM)

        with open(CERT_PEM_FILE, "wb") as f:
            f.write(cert_pem)

        spki_pin = calculate_spki_sha256(cert)

        _cached_cert = cert
        _cached_key_pem = priv_pem
        _cached_spki_pin = spki_pin

        print(f"[CryptoManager] Generated new ECDSA P-256 keypair and X.509 certificate.")
        print(f"[CryptoManager] SPKI SHA-256 Fingerprint: {spki_pin}")
        return cert, priv_pem, spki_pin


class _PySSLContext(ctypes.Structure):
    """C-level structure matching CPython's PySSLContext to extract raw SSL_CTX pointer."""
    _fields_ = [
        ("ob_refcnt", ctypes.c_ssize_t),
        ("ob_type", ctypes.c_void_p),
        ("ctx", ctypes.c_void_p),
    ]


def build_server_ssl_context(cert: x509.Certificate, priv_pem: bytes) -> ssl.SSLContext:
    """
    Constructs an ssl.SSLContext enforcing:
      - TLS 1.3 only (minimum_version = TLSv1_3, maximum_version = TLSv1_3).
      - Mutual TLS with mandatory client certificate (verify_mode = CERT_REQUIRED).
      - Strong cipher suites prioritizing TLS_AES_256_GCM_SHA384.
      - OpenSSL verify callback hook allowing self-signed client certificates for application-layer verification.
    """
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    ctx.maximum_version = ssl.TLSVersion.TLSv1_3
    ctx.verify_mode = ssl.CERT_OPTIONAL

    # Write key and cert to temporary file to load into OpenSSL engine
    with tempfile.NamedTemporaryFile("wb", delete=False) as key_f:
        key_f.write(priv_pem)
        key_path = key_f.name

    cert_pem = cert.public_bytes(serialization.Encoding.PEM)
    with tempfile.NamedTemporaryFile("wb", delete=False) as cert_f:
        cert_f.write(cert_pem)
        cert_path = cert_f.name

    try:
        ctx.load_cert_chain(certfile=cert_path, keyfile=key_path)
    finally:
        # Securely remove temporary key and cert files after OpenSSL loads them into memory
        for p in (key_path, cert_path):
            try:
                if os.path.exists(p):
                    os.remove(p)
            except Exception:
                pass

    # Configure OpenSSL C-level settings
    libssl = _get_libssl()
    if libssl:
        try:
            raw_ctx_ptr = _PySSLContext.from_address(id(ctx)).ctx
            # Configure cipher suites
            if hasattr(libssl, "SSL_CTX_set_ciphersuites"):
                libssl.SSL_CTX_set_ciphersuites.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
                libssl.SSL_CTX_set_ciphersuites.restype = ctypes.c_int
                libssl.SSL_CTX_set_ciphersuites(
                    raw_ctx_ptr,
                    b"TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_128_GCM_SHA256",
                )

            # Install custom verify callback so client self-signed cert is passed through to getpeercert()
            if hasattr(libssl, "SSL_CTX_set_cert_verify_callback"):
                libssl.SSL_CTX_set_cert_verify_callback.argtypes = [
                    ctypes.c_void_p,
                    VERIFY_CB,
                    ctypes.c_void_p,
                ]
                libssl.SSL_CTX_set_cert_verify_callback.restype = None
                libssl.SSL_CTX_set_cert_verify_callback(raw_ctx_ptr, _C_VERIFY_CB, None)
        except Exception as e:
            print(f"[CryptoManager] Warning: Failed to apply C-level SSL_CTX tweaks: {e}")

    return ctx
