//! Export HTML (port de `Contest::DumpToHTML`, `Schedule::DumpToHTML` et
//! `PlayersList::DumpToHTML`).

use crate::attribute::{registry, Look, Scope, Value};
use crate::filter::Filter;
use crate::player::{AttrId, Owner, PlayerRef, PlayerStore};

/// Feuille de style et script embarqués (`css.h`).
pub const CSS: &str = include_str!("html.css");

fn escape(s: &str) -> String {
    s.to_string()
}

/// Image utilisateur d'une valeur (`GetUserImage (LONG_TEXT)`).
pub fn user_image(code: &str, v: &Value) -> String {
    match registry().by_code(code) {
        Some(d) => d.user_image(&v.xml_image(), Look::LongText),
        None => v.xml_image(),
    }
}

/// `IsTableBorder` du classement : une ligne de séparation avant chaque
/// puissance de 2 (> 1).
pub fn is_power_of_two_border(place: usize) -> bool {
    place > 1 && place.is_power_of_two()
}

/// Tableau d'une liste de joueurs (sans séparateurs).
pub fn players_table(html: &mut String, store: &PlayerStore, list: &[PlayerRef], filter: &Filter, owner: Owner) {
    players_table_with_borders(html, store, list, filter, owner, |_| false)
}

/// `PlayersList::DumpToHTML`.
pub fn players_table_with_borders(
    html: &mut String,
    store: &PlayerStore,
    list: &[PlayerRef],
    filter: &Filter,
    owner: Owner,
    is_border: impl Fn(usize) -> bool,
) {
    html.push_str("        <table class=\"List\">\n");
    html.push_str("          <tr class=\"ListHeader\">\n");
    for l in filter.layout() {
        let user = registry().by_code(&l.code_name).map(|d| d.user_name).unwrap_or(&l.code_name);
        html.push_str(&format!("            <th>{}</th>\n", escape(user)));
    }
    html.push_str("          </tr>\n");
    for (i, r) in list.iter().enumerate() {
        let Some(p) = store.get(*r) else { continue };
        if is_border(i) {
            html.push_str("          <tr class=\"Separator\">\n");
        }
        html.push_str(if i % 2 == 1 { "          <tr class=\"OddRow\">\n" } else { "          <tr class=\"EvenRow\">\n" });
        for l in filter.layout() {
            let local = registry().by_code(&l.code_name).map(|d| d.scope == Scope::Local).unwrap_or(false);
            let id = if local { AttrId::local(&l.code_name, owner) } else { AttrId::global(&l.code_name) };
            match p.get(&id) {
                Some(v) => html.push_str(&format!("            <td>{}</td>\n", escape(&user_image(&l.code_name, v)))),
                None => html.push_str("            <td></td>\n"),
            }
        }
        html.push_str("          </tr>\n");
    }
    html.push_str("        </table>\n");
}

/// En-tête du document (`Contest::DumpToHTML`).
pub fn header(html: &mut String, name: &str, date: &str, weapon: &str, gender: &str, category: &str) {
    html.push_str(&format!(
        "<html>\n  <head>\n    <Title>{name} {date} - {weapon} - {gender} - {category}</Title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\"/>\n{CSS}  </head>\n\n"
    ));
    html.push_str(&format!(
        "  <body onLoad=\"javascript:OnLoad ();\">\n    <div id='main_div'>\n      <div class=\"Title\">\n        <center>\n          <h1>{name}</h1>\n          <h1>{date}</h1>\n          <h1>{weapon} - {gender} - {category}</h1>\n        </center>\n      <div>\n\n"
    ));
}

/// Pied du document.
pub fn footer(html: &mut String) {
    html.push_str("      <div class=\"Footer\">\n        <h6>Managed by ManageComp, derived from BellePoule <a href=\"https://managecomp.org\">https://managecomp.org</a><h6/>\n      <div>\n    </div>\n  </body>\n");
    html.push_str("</html>\n");
}
