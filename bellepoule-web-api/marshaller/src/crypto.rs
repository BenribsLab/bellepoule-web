//! Chiffrement des mots de passe d'arbitre (port de `network/cryptor.cpp`) :
//! AES-256-CBC, bourrage PKCS#7, texte terminé par un octet nul, base64.

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::{engine::general_purpose::STANDARD, Engine};

type Enc = cbc::Encryptor<aes::Aes256>;
type Dec = cbc::Decryptor<aes::Aes256>;

/// IV nul utilisé pour les arbitres (`Referee::_iv`), en base64.
pub fn zero_iv_b64() -> String {
    STANDARD.encode([0u8; 16])
}

fn key32(key: &str) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (d, s) in k.iter_mut().zip(key.as_bytes()) {
        *d = *s;
    }
    k
}

fn iv16(iv_b64: &str) -> [u8; 16] {
    let mut iv = [0u8; 16];
    if let Ok(bytes) = STANDARD.decode(iv_b64) {
        for (d, s) in iv.iter_mut().zip(bytes.iter()) {
            *d = *s;
        }
    }
    iv
}

/// `Cryptor::Encrypt`.
pub fn encrypt(text: &str, key: &str, iv_b64: &str) -> String {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    let cipher = Enc::new(&key32(key).into(), &iv16(iv_b64).into()).encrypt_padded_vec_mut::<Pkcs7>(&data);
    STANDARD.encode(cipher)
}

/// `Cryptor::Decrypt`.
pub fn decrypt(data_b64: &str, iv_b64: &str, key: &str) -> Option<String> {
    let bytes = STANDARD.decode(data_b64.trim()).ok()?;
    let plain = Dec::new(&key32(key).into(), &iv16(iv_b64).into()).decrypt_padded_vec_mut::<Pkcs7>(&bytes).ok()?;
    let end = plain.iter().position(|b| *b == 0).unwrap_or(plain.len());
    String::from_utf8(plain[..end].to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = "0123456789abcdefghijABCDEFGHIJKL";
        let c = encrypt("secret password", key, &zero_iv_b64());
        assert_eq!(decrypt(&c, &zero_iv_b64(), key).unwrap(), "secret password");
        assert!(decrypt(&c, &zero_iv_b64(), "wrong key wrong key wrong key xx").map(|s| s != "secret password").unwrap_or(true));
    }
}
