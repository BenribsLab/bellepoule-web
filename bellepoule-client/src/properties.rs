//! Propriétés d'une compétition (création et modification), avec les listes
//! de l'application : armes, sexes, catégories, niveaux.

use leptos::prelude::*;

use crate::api::{self, Meta, Properties};

/// Liste déroulante liée à un signal.
#[component]
pub fn Select(choices: Vec<api::Choice>, value: RwSignal<String>) -> impl IntoView {
    view! {
        <select on:change=move |ev| value.set(event_target_value(&ev)) prop:value=move || value.get()>
            {choices
                .into_iter()
                .map(|c| {
                    let v = c.value.clone();
                    view! { <option value=c.value selected=move || value.get() == v>{c.label}</option> }
                })
                .collect_view()}
        </select>
    }
}

/// Champs des propriétés, liés à des signaux.
#[derive(Clone, Copy)]
pub struct PropertiesForm {
    pub name: RwSignal<String>,
    pub organizer: RwSignal<String>,
    pub location: RwSignal<String>,
    pub label: RwSignal<String>,
    pub web_site: RwSignal<String>,
    pub weapon: RwSignal<String>,
    pub gender: RwSignal<String>,
    pub category: RwSignal<String>,
    pub level: RwSignal<String>,
    pub date: RwSignal<String>,
    pub color: RwSignal<String>,
    pub team_event: RwSignal<bool>,
}

impl PropertiesForm {
    pub fn new(p: &Properties) -> Self {
        PropertiesForm {
            name: RwSignal::new(p.name.clone()),
            organizer: RwSignal::new(p.organizer.clone()),
            location: RwSignal::new(p.location.clone()),
            label: RwSignal::new(p.label.clone()),
            web_site: RwSignal::new(p.web_site.clone()),
            weapon: RwSignal::new(p.weapon.clone()),
            gender: RwSignal::new(p.gender.clone()),
            category: RwSignal::new(p.category.to_string()),
            level: RwSignal::new(p.level.to_string()),
            date: RwSignal::new(iso_date(&p.date)),
            color: RwSignal::new(p.color.clone()),
            team_event: RwSignal::new(p.team_event),
        }
    }

    pub fn read(&self) -> Properties {
        Properties {
            name: self.name.get(),
            organizer: self.organizer.get(),
            location: self.location.get(),
            label: self.label.get(),
            web_site: self.web_site.get(),
            weapon: self.weapon.get(),
            gender: self.gender.get(),
            category: self.category.get().parse().unwrap_or(0),
            level: self.level.get().parse().unwrap_or(0),
            date: self.date.get(),
            color: self.color.get(),
            team_event: self.team_event.get(),
        }
    }
}

/// `jj/mm/aaaa` → `aaaa-mm-jj` (champ date du navigateur).
fn iso_date(d: &str) -> String {
    let parts: Vec<&str> = d.split('/').collect();
    match parts[..] {
        [day, month, year] => format!("{year}-{month}-{day}"),
        _ => d.to_string(),
    }
}

/// Valeurs par défaut d'une nouvelle compétition : date du jour.
pub fn new_properties() -> Properties {
    let today = js_sys::Date::new_0();
    Properties {
        weapon: "E".into(),
        gender: "M".into(),
        date: format!("{:02}/{:02}/{}", today.get_date(), today.get_month() + 1, today.get_full_year()),
        ..Properties::default()
    }
}

fn text(label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label>
            {label}
            <input type="text" prop:value=move || value.get() on:input=move |ev| value.set(event_target_value(&ev)) />
        </label>
    }
}

/// Champs du formulaire (sans les boutons).
#[component]
pub fn PropertiesFields(form: PropertiesForm, meta: Meta, #[prop(default = true)] team_editable: bool) -> impl IntoView {
    view! {
        <div class="fields">
            <label class="wide">
                "Nom"
                <input
                    type="text"
                    required
                    prop:value=move || form.name.get()
                    on:input=move |ev| form.name.set(event_target_value(&ev))
                />
            </label>
            <label>"Arme" <Select choices=meta.weapons.clone() value=form.weapon /></label>
            <label>"Sexe" <Select choices=meta.genders.clone() value=form.gender /></label>
            <label>"Catégorie" <Select choices=meta.categories.clone() value=form.category /></label>
            <label>"Niveau" <Select choices=meta.levels.clone() value=form.level /></label>
            <label>
                "Date"
                <input type="date" prop:value=move || form.date.get() on:input=move |ev| form.date.set(event_target_value(&ev)) />
            </label>
            {text("Lieu", form.location)}
            {text("Organisateur", form.organizer)}
            {text("Libellé", form.label)}
            {text("Site web", form.web_site)}
            <label>
                "Couleur"
                <select on:change=move |ev| form.color.set(event_target_value(&ev)) prop:value=move || form.color.get()>
                    {meta
                        .colors
                        .iter()
                        .map(|c| {
                            let c2 = c.clone();
                            let style = format!("background:{c}");
                            view! { <option value=c.clone() style=style selected=move || form.color.get() == c2>{c.clone()}</option> }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    disabled=!team_editable
                    prop:checked=move || form.team_event.get()
                    on:change=move |ev| form.team_event.set(event_target_checked(&ev))
                />
                "Épreuve par équipes"
            </label>
        </div>
    }
}
