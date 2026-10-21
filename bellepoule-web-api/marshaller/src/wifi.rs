//! Codes QR de configuration des smartphones (port de `util/wifi_code.cpp`,
//! `util/flash_code.cpp`, `network/wifi_network.cpp` et `smartphones.cpp`).

use qrcode::{EcLevel, QrCode};
use rand::Rng;

/// Réseau WIFI saisi dans le dialogue « WIFI ».
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WifiNetwork {
    pub ssid: String,
    pub passphrase: String,
    pub encryption: String,
}

impl WifiNetwork {
    /// `WifiNetwork::IsValid`.
    pub fn is_valid(&self) -> bool {
        !self.ssid.is_empty()
    }

    /// `WifiCode::GetNetwork`.
    pub fn code_prefix(&self) -> String {
        if self.is_valid() {
            format!("[{}-{}-{}]", self.ssid, self.encryption, self.passphrase)
        } else {
            String::new()
        }
    }
}

/// Port d'écoute annoncé aux smartphones (`WifiCode::ClaimIpPort`, 35830‥35839).
pub const FIRST_PORT: u16 = 35830;

/// `FlashCode::GetKey256` : 32 caractères parmi chiffres, majuscules et
/// minuscules (bornes supérieures exclues, comme `g_rand_int_range`).
pub fn new_key256() -> String {
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| match rng.gen_range(0..3) {
            0 => rng.gen_range(b'0'..b'9') as char,
            1 => rng.gen_range(b'A'..b'Z') as char,
            _ => rng.gen_range(b'a'..b'z') as char,
        })
        .collect()
}

/// Texte du QR code d'un utilisateur (`WifiCode::GetText`) : un arbitre
/// (`name`, `reference`) ou l'administrateur (`reference` = 0).
pub fn wifi_code_text(network: &WifiNetwork, ip: &str, port: u16, name: &str, reference: u32, key: &str) -> String {
    format!("{}{}:{}-{}-{}-{}", network.code_prefix(), ip, port, name, reference, key)
}

/// Matrice d'un QR code (niveau de correction L), avec la marge de 4 modules
/// de `FlashCode::GetPixbuf`. `true` = module noir.
pub fn qr_matrix(text: &str) -> Vec<Vec<bool>> {
    const MARGIN: usize = 4;
    let Ok(code) = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L) else {
        return Vec::new();
    };
    let w = code.width();
    let colors = code.to_colors();
    let size = w + 2 * MARGIN;
    let mut m = vec![vec![false; size]; size];
    for y in 0..w {
        for x in 0..w {
            m[y + MARGIN][x + MARGIN] = colors[y * w + x] == qrcode::Color::Dark;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_and_text() {
        let k = new_key256();
        assert_eq!(k.len(), 32);
        assert!(k.chars().all(|c| c.is_ascii_alphanumeric()));
        let net = WifiNetwork { ssid: "Salle".into(), passphrase: "pw".into(), encryption: "WPA".into() };
        assert_eq!(wifi_code_text(&net, "192.168.1.2", 35830, "Administrator", 0, "K"), "[Salle-WPA-pw]192.168.1.2:35830-Administrator-0-K");
        assert_eq!(wifi_code_text(&WifiNetwork::default(), "ip", 1, "A", 3, "K"), "ip:1-A-3-K");
        let m = qr_matrix("hello");
        assert_eq!(m.len(), 21 + 8);
        assert!(m[4][4]);
        assert!(!m[0][0]);
    }
}
