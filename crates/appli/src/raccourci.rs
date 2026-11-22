//! Un raccourci valable partout pour arrêter de filmer : le portail GlobalShortcuts de KDE. Il ne
//! se demande qu'au premier enregistrement, KDE fait alors confirmer la touche. Sous Windows, la
//! même touche (Windows+F12) par `RegisterHotKey`.

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

/// Windows : la touche appartient au fil qui l'enregistre, ses appuis arrivent dans sa file de messages.
#[cfg(windows)]
pub fn lancer() -> Receiver<Evenement> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, MOD_WIN, RegisterHotKey, VK_F12};
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
    let (envoi, recu) = mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY : un raccourci et une file de messages propres à ce fil.
        if let Err(e) = unsafe { RegisterHotKey(None, 1, MOD_WIN | MOD_NOREPEAT, VK_F12.0 as u32) } {
            let _ = envoi.send(Evenement::Echec(format!("Windows+F12 est déjà pris ({e})")));
            return;
        }
        let _ = envoi.send(Evenement::Pret);
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            if message.message == WM_HOTKEY && envoi.send(Evenement::Appui).is_err() {
                break;
            }
        }
    });
    recu
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn lancer() -> Receiver<Evenement> {
    let (envoi, recu) = mpsc::channel();
    let _ = envoi.send(Evenement::Echec("pas de raccourci global sur ce système".into()));
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
