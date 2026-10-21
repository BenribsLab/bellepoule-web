//! BellePoule Web — API.
//!
//! Variables d'environnement :
//! - `BP_DATA_DIR` : dossier des compétitions (`/data` par défaut) ;
//! - `BP_RESOURCES` : ressources de BellePoule (`/app/resources` par défaut) ;
//! - `BP_LANGUAGE` : langue des libellés (`fr` par défaut) ;
//! - `BP_ADDRESS` : adresse d'écoute (`0.0.0.0:8080` par défaut) ;
//! - `BP_ADMIN_KEY` : clé d'administration (création des clés des clients).

mod api;
mod auth;
mod checkin;
mod contests;
mod organisation;
mod schedule;
mod smartpoule;
mod stages;
mod store;
mod view;

use std::path::PathBuf;
use std::sync::Arc;

use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=info".into()))
        .init();

    // Ressources de BellePoule : pays, ligues, clubs, traductions.
    let resources = PathBuf::from(env_or("BP_RESOURCES", "/app/resources"));
    let language = env_or("BP_LANGUAGE", "fr");
    bp_core::contest::players::set_resources_dir(resources.clone());
    bp_core::formats::discrete::init(&resources, &language);
    if let Err(e) = bp_core::i18n::set_language(&resources.join("translations"), &language) {
        tracing::warn!("traductions « {language} » : {e}");
    }

    let store = store::Store::open(PathBuf::from(env_or("BP_DATA_DIR", "/data")))?;
    if std::env::var("BP_ADMIN_KEY").map(|k| k.len() < 16).unwrap_or(true) {
        tracing::warn!("BP_ADMIN_KEY absente ou trop courte : la création des clés des clients est désactivée");
    }
    let app = api::router(Arc::new(tokio::sync::Mutex::new(store)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let address = env_or("BP_ADDRESS", "0.0.0.0:8080");
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("BellePoule Web API à l'écoute sur {address}");
    axum::serve(listener, app).await
}
