//! Un raccourci valable partout pour arrêter de filmer : le portail GlobalShortcuts de KDE. Il ne
//! se demande qu'au premier enregistrement, KDE fait alors confirmer la touche.

use std::sync::mpsc::{self, Receiver, Sender};

use ashpd::AppID;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;

/// `LOGO` : la touche Windows. Ctrl+Alt+Fn changerait de console, et aucun logiciel filmé ne s'en sert.
const TOUCHE: &str = "LOGO+F12";

pub enum Evenement {
    Pret,
    Appui,
    Echec(String),
}

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
