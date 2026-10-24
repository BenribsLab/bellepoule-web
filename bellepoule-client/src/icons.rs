//! Icônes en ligne (traits fins, 24 × 24), sans ressource extérieure.

use leptos::prelude::*;

fn paths(name: &str) -> &'static str {
    match name {
        "trophy" => r#"<path d="M8 21h8M12 17v4M7 4h10v5a5 5 0 0 1-10 0V4z"/><path d="M17 5h3v2a3 3 0 0 1-3 3M7 5H4v2a3 3 0 0 0 3 3"/>"#,
        "grid" => r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#,
        "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "upload" => r#"<path d="M12 15V3M7 8l5-5 5 5"/><path d="M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4"/>"#,
        "download" => r#"<path d="M12 3v12M7 10l5 5 5-5"/><path d="M4 15v4a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-4"/>"#,
        "logout" => r#"<path d="M15 4h3a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3M10 17l5-5-5-5M15 12H3"/>"#,
        "settings" => r#"<path d="M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>"#,
        "flow" => r#"<circle cx="5" cy="6" r="2"/><circle cx="5" cy="18" r="2"/><circle cx="19" cy="12" r="2"/><path d="M7 6h4a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H7M13 12h4"/>"#,
        "chevron-left" => r#"<path d="M15 18l-6-6 6-6"/>"#,
        "chevron-right" => r#"<path d="M9 18l6-6-6-6"/>"#,
        "calendar" => r#"<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>"#,
        "pin" => r#"<path d="M12 21s7-6.2 7-12a7 7 0 0 0-14 0c0 5.8 7 12 7 12z"/><circle cx="12" cy="9" r="2.5"/>"#,
        "users" => r#"<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M16 4.5a3.5 3.5 0 0 1 0 7M18 14a6 6 0 0 1 3.5 6"/>"#,
        "trash" => r#"<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>"#,
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>"#,
        "check" => r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#,
        "alert" => r#"<path d="M12 9v4M12 17h.01"/><path d="M10.3 3.9L2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>"#,
        "lock" => r#"<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>"#,
        "key" => r#"<circle cx="8" cy="15" r="4"/><path d="M11 12l9-9M17 6l3 3M14 9l2 2"/>"#,
        "blade" => r#"<path d="M4 20L18 6M15 4l5 5M4 20l-1 1M6 15l3 3"/>"#,
        "shield" => r#"<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6l8-3z"/><path d="M9 12l2 2 4-4"/>"#,
        "mail" => r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 7l9 6 9-6"/>"#,
        "user" => r#"<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>"#,
        "copy" => r#"<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#,
        "building" => r#"<path d="M4 21V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v16M16 9h2a2 2 0 0 1 2 2v10M8 7h4M8 11h4M8 15h4M3 21h18"/>"#,
        "activity" => r#"<path d="M3 12h4l3-8 4 16 3-8h4"/>"#,
        "x" => r#"<path d="M6 6l12 12M18 6L6 18"/>"#,
        "folder" => r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z"/>"#,
        "file-plus" => r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8l-5-5z"/><path d="M14 3v5h5M12 11v6M9 14h6"/>"#,
        _ => "",
    }
}

/// Icône à traits, de la couleur du texte.
#[component]
pub fn Icon(name: &'static str) -> impl IntoView {
    let svg = format!(
        r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{}</svg>"#,
        paths(name)
    );
    view! { <span class="icon" inner_html=svg></span> }
}

/// Logo ManageComp (arbre de tableau).
#[component]
pub fn Logo() -> impl IntoView {
    view! { <img class="logo" src="/logo.png" alt="" width="256" height="256" /> }
}

/// Logo et titre « ManageComp ».
#[component]
pub fn Header() -> impl IntoView {
    view! { <img class="brand-header" src="/header.png" alt="ManageComp" width="524" height="160" /> }
}
