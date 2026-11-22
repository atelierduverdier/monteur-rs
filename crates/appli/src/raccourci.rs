//! Un raccourci valable partout pour arrêter de filmer : le portail GlobalShortcuts de KDE. Il ne
//! se demande qu'au premier enregistrement, KDE fait alors confirmer la touche. Sous Linux
//! seulement pour l'instant : Windows aura le sien avec sa capture.

use std::sync::mpsc::{self, Receiver};
#[cfg(target_os = "linux")]
use std::sync::mpsc::Sender;

#[cfg(target_os = "linux")]
use ashpd::AppID;
#[cfg(target_os = "linux")]
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
#[cfg(target_os = "linux")]
use futures_util::StreamExt;

/// `LOGO` : la touche Windows. Ctrl+Alt+Fn changerait de console, et aucun logiciel filmé ne s'en sert.
#[cfg(target_os = "linux")]
const TOUCHE: &str = "LOGO+F12";

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Evenement {
    Pret,
    Appui,
    Echec(String),
}

#[cfg(target_os = "linux")]
async fn ecouter(envoi: &Sender<Evenement>) -> Result<(), String> {
    let texte = |e: ashpd::Error| e.to_string();
    let id: AppID = "fr.atelierduverdier.Monteur2".try_into().map_err(|e: ashpd::Error| e.to_string())?;
    // Refusé si le système ne sait pas faire : le portail se débrouille alors sans identité.
    if let Err(e) = ashpd::register_host_app(id).await {
        eprintln!("portail : identité refusée : {e}");
    }
    let portail = GlobalShortcuts::new().await.map_err(texte)?;
    let session = portail.create_session(Default::default()).await.map_err(texte)?;
    let touche = NewShortcut::new("arreter", "Arrêter de filmer").preferred_trigger(TOUCHE);
    portail.bind_shortcuts(&session, &[touche], None, BindShortcutsOptions::default()).await.map_err(texte)?.response().map_err(texte)?;
    let _ = envoi.send(Evenement::Pret);
    let mut appuis = portail.receive_activated().await.map_err(texte)?;
    while appuis.next().await.is_some() {
        if envoi.send(Evenement::Appui).is_err() {
            break;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn lancer() -> Receiver<Evenement> {
    let (envoi, recu) = mpsc::channel();
    let _ = envoi.send(Evenement::Echec("pas encore de raccourci global sur ce système".into()));
    recu
}

#[cfg(target_os = "linux")]
pub fn lancer() -> Receiver<Evenement> {
    let (envoi, recu) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok(moteur) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
        if let Err(e) = moteur.block_on(ecouter(&envoi)) {
            let _ = envoi.send(Evenement::Echec(e));
        }
    });
    recu
}
