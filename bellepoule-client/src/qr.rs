//! QR codes de ManageComp Arbitre, dessinés dans le navigateur (SVG).
//!
//! - QR code personnel d'un arbitre : `?c=<club>&r=<arbitre>&k=<clé>`,
//!   sans PIN (remis en main propre) ;
//! - QR code d'une feuille (poule, match) : `?c=<club>&s=<jeton>`, qui ne
//!   donne rien sans le PIN d'un arbitre affecté à la feuille.

use leptos::prelude::*;

fn enc(text: &str) -> String {
    js_sys::encode_uri_component(text).into()
}

/// Adresse de ManageComp Arbitre : `arbitre.<domaine>` quand l'application
/// est servie sur `app.<domaine>`, sinon `/arbitre/` sur le même site.
pub fn arbitre_base() -> String {
    let Some(location) = web_sys::window().map(|w| w.location()) else { return "/arbitre/".into() };
    let protocol = location.protocol().unwrap_or_else(|_| "https:".into());
    let host = location.host().unwrap_or_default();
    match host.strip_prefix("app.") {
        Some(domain) => format!("{protocol}//arbitre.{domain}/"),
        None => format!("{}/arbitre/", location.origin().unwrap_or_default()),
    }
}

/// QR code d'une feuille de poule ou de match.
pub fn sheet_url(club: &str, token: &str) -> String {
    format!("{}?c={}&s={}", arbitre_base(), enc(club), enc(token))
}

/// QR code personnel d'un arbitre (`reference = 0` : administrateur).
pub fn personal_url(club: &str, reference: u64, key: &str) -> String {
    format!("{}?c={}&r={reference}&k={}", arbitre_base(), enc(club), enc(key))
}

/// SVG du QR code (vide si le texte est trop long).
pub fn svg(text: &str) -> String {
    use qrcode::render::svg;
    qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M)
        .map(|code| code.render::<svg::Color>().quiet_zone(true).min_dimensions(160, 160).dark_color(svg::Color("#000")).light_color(svg::Color("#fff")).build())
        .unwrap_or_default()
}

/// QR code ; sa taille suit celle de son conteneur (`.qr`).
#[component]
pub fn QrCode(text: String) -> impl IntoView {
    view! { <span class="qr" inner_html=svg(&text)></span> }
}
