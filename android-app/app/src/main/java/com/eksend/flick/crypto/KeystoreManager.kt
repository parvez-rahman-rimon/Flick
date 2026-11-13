package com.eksend.flick.crypto

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.math.BigInteger
import java.net.Socket
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.Principal
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.security.spec.ECGenParameterSpec
import java.util.Date
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLEngine
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.X509ExtendedKeyManager
import javax.net.ssl.X509TrustManager
import javax.security.auth.x500.X500Principal

/**
 * Flick Android Keystore & Mutual TLS 1.3 Manager
 * - Generates & protects ECDSA P-256 (secp256r1) keypair inside AndroidKeyStore (StrongBox with TEE fallback).
 * - Exports self-signed X.509 client certificate for mTLS client authentication.
 * - Computes HMAC-SHA256 proofs of possession for pairing.
 * - Builds custom X509KeyManager and PinningTrustManager for OkHttp TLS 1.3 socket factory.
 * - Validates rolling UDP beacon authenticator tags (±1 time step).
 */
object KeystoreManager {

    private const val ANDROID_KEYSTORE = "AndroidKeyStore"
    private const val KEY_ALIAS = "flick_client_ecdsa_v3"

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

    fun ensureClientKeypair(clientId: String) {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        try {
            if (keyStore.containsAlias("flick_client_ecdsa_v2")) {
                keyStore.deleteEntry("flick_client_ecdsa_v2")
            }
            if (keyStore.containsAlias("flick_client_ecdsa_v1")) {
                keyStore.deleteEntry("flick_client_ecdsa_v1")
            }
        } catch (_: Exception) {}

        if (keyStore.containsAlias(KEY_ALIAS)) {
            return
        }

        val subject = X500Principal("CN=Flick-Android-$clientId")
        val now = System.currentTimeMillis()
        val notBefore = Date(now - 24 * 3600 * 1000L)
        val notAfter = Date(now + 10L * 365 * 24 * 3600 * 1000L) // 10 years

        var success = false
        // Try hardware StrongBox first if supported (Android 9+ / API 28+)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            try {
                val strongBoxSpec = KeyGenParameterSpec.Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
                )
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(
                        KeyProperties.DIGEST_NONE,
                        KeyProperties.DIGEST_SHA256,
                        KeyProperties.DIGEST_SHA384,
                        KeyProperties.DIGEST_SHA512
                    )
                    .setCertificateSubject(subject)
                    .setCertificateSerialNumber(BigInteger.valueOf(now))
                    .setCertificateNotBefore(notBefore)
                    .setCertificateNotAfter(notAfter)
                    .setIsStrongBoxBacked(true)
                    .build()

                val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE)
                kpg.initialize(strongBoxSpec)
                kpg.generateKeyPair()
                success = true
            } catch (_: Throwable) {
                // Fall through to standard hardware TEE
            }
        }

        if (!success) {
            val teeSpec = KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
            )
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(
                    KeyProperties.DIGEST_NONE,
                    KeyProperties.DIGEST_SHA256,
                    KeyProperties.DIGEST_SHA384,
                    KeyProperties.DIGEST_SHA512
                )
                .setCertificateSubject(subject)
                .setCertificateSerialNumber(BigInteger.valueOf(now))
                .setCertificateNotBefore(notBefore)
                .setCertificateNotAfter(notAfter)
                .build()

            val kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE)
            kpg.initialize(teeSpec)
            kpg.generateKeyPair()
        }
    }

    fun getClientCertificate(): X509Certificate {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        return (keyStore.getCertificate(KEY_ALIAS) as? X509Certificate)
            ?: throw IllegalStateException("Client certificate not found in Keystore")
    }

    fun getClientPrivateKey(): PrivateKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        return (keyStore.getKey(KEY_ALIAS, null) as? PrivateKey)
            ?: throw IllegalStateException("Client private key not found in Keystore")
    }

    fun getClientSpkiPin(): String {
        val cert = getClientCertificate()
        val spkiBytes = cert.publicKey.encoded
        return MessageDigest.getInstance("SHA-256").digest(spkiBytes).toHex().lowercase()
    }

    fun getClientCertPin(): String {
        val cert = getClientCertificate()
        return MessageDigest.getInstance("SHA-256").digest(cert.encoded).toHex().lowercase()
    }

    fun generateProofOfPossession(pairSecret: String): String {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(pairSecret.toByteArray(Charsets.UTF_8), "HmacSHA256"))
        val certDer = getClientCertificate().encoded
        return mac.doFinal(certDer).toHex().lowercase()
    }

    class FlickX509KeyManager(
        private val alias: String,
        private val privateKey: PrivateKey,
        private val certChain: Array<X509Certificate>
    ) : X509ExtendedKeyManager() {
        override fun getClientAliases(keyType: String?, issuers: Array<out Principal>?): Array<String> = arrayOf(alias)
        override fun chooseClientAlias(keyType: Array<out String>?, issuers: Array<out Principal>?, socket: Socket?): String = alias
        override fun getServerAliases(keyType: String?, issuers: Array<out Principal>?): Array<String>? = null
        override fun chooseServerAlias(keyType: String?, issuers: Array<out Principal>?, socket: Socket?): String? = null
        override fun getCertificateChain(alias: String?): Array<X509Certificate>? = if (alias == this.alias) certChain else null
        override fun getPrivateKey(alias: String?): PrivateKey? = if (alias == this.alias) privateKey else null

        override fun chooseEngineClientAlias(
            keyType: Array<out String>?,
            issuers: Array<out Principal>?,
            engine: SSLEngine?
        ): String = alias

        override fun chooseEngineServerAlias(
            keyType: String?,
            issuers: Array<out Principal>?,
            engine: SSLEngine?
        ): String? = null
    }

    class PinningTrustManager(private val expectedPin: String) : X509TrustManager {
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {}

        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
            if (chain.isNullOrEmpty()) {
                throw CertificateException("Server certificate chain is empty")
            }
            val serverCert = chain[0]

            // Calculate SPKI SHA-256
            val spkiBytes = serverCert.publicKey.encoded
            val spkiSha256 = MessageDigest.getInstance("SHA-256").digest(spkiBytes).toHex().lowercase()

            // Calculate Cert DER SHA-256
            val certSha256 = MessageDigest.getInstance("SHA-256").digest(serverCert.encoded).toHex().lowercase()

            val target = expectedPin.trim().lowercase()
            if (target.isNotEmpty() && target != spkiSha256 && target != certSha256) {
                throw CertificateException(
                    "Certificate pin mismatch! Expected $target but got SPKI: $spkiSha256 / Cert: $certSha256"
                )
            }
        }

        override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
    }

    fun createTlsConfig(pcSpkiPin: String): Pair<SSLSocketFactory, X509TrustManager> {
        val clientCert = getClientCertificate()
        val privateKey = getClientPrivateKey()

        val keyManager = FlickX509KeyManager(
            alias = KEY_ALIAS,
            privateKey = privateKey,
            certChain = arrayOf(clientCert)
        )
        val trustManager = PinningTrustManager(pcSpkiPin)

        val sslContext = SSLContext.getInstance("TLSv1.3")
        sslContext.init(arrayOf(keyManager), arrayOf(trustManager), SecureRandom())

        return Pair(sslContext.socketFactory, trustManager)
    }

    fun verifyRollingBeaconTag(
        pairSecret: String,
        pcId: String,
        receivedTag: String,
        authTags: Map<String, String>?,
        clientId: String
    ): Boolean {
        if (pairSecret.isEmpty()) return true
        val epochSeconds = System.currentTimeMillis() / 1000
        val currentStep = epochSeconds / 30

        val clientSpecificTag = authTags?.get(clientId) ?: ""

        for (offset in -1L..1L) {
            val step = currentStep + offset
            val msg = "$step$pcId".toByteArray(Charsets.UTF_8)
            val mac = Mac.getInstance("HmacSHA256")
            mac.init(SecretKeySpec(pairSecret.toByteArray(Charsets.UTF_8), "HmacSHA256"))
            val expected = mac.doFinal(msg).toHex().lowercase()

            if (expected.equals(receivedTag.trim().lowercase(), ignoreCase = true) ||
                (clientSpecificTag.isNotEmpty() && expected.equals(clientSpecificTag.trim().lowercase(), ignoreCase = true))) {
                return true
            }
        }
        return false
    }
}
