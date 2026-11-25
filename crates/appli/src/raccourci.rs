//! Un raccourci valable partout pour arrêter de filmer : le portail GlobalShortcuts de KDE. Il ne
//! se demande qu'au premier enregistrement, KDE fait alors confirmer la touche. Sous Windows, la
//! même touche (Windows+F12) par `RegisterHotKey`.
//!
//! LA WEBCAM EN GRAND PENDANT LE TOURNAGE (25/11/2026) : Meta+F11 (Windows+F11) la fait passer en grand, puis la
//! ramène ; le montage en reçoit les passages. KDE fait confirmer cette touche-là aussi, une fois.

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
#[cfg(target_os = "linux")]
const TOUCHE_WEBCAM: &str = "LOGO+F11";

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Evenement {
    Pret,
    /// Arrêter de filmer.
    Appui,
    /// La webcam en grand, ou de retour en vignette.
    Webcam,
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
    let webcam = NewShortcut::new("webcam", "Webcam en grand ou en vignette").preferred_trigger(TOUCHE_WEBCAM);
    portail.bind_shortcuts(&session, &[touche, webcam], None, BindShortcutsOptions::default()).await.map_err(texte)?.response().map_err(texte)?;
    let _ = envoi.send(Evenement::Pret);
    let mut appuis = portail.receive_activated().await.map_err(texte)?;
    while let Some(a) = appuis.next().await {
        let e = if a.shortcut_id() == "webcam" { Evenement::Webcam } else { Evenement::Appui };
        if envoi.send(e).is_err() {
            break;
        }
    }
    Ok(())
}

/// Windows : la touche appartient au fil qui l'enregistre, ses appuis arrivent dans sa file de messages.
#[cfg(windows)]
pub fn lancer() -> Receiver<Evenement> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, MOD_WIN, RegisterHotKey, VK_F11, VK_F12};
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
    let (envoi, recu) = mpsc::channel();
    std::thread::spawn(move || {
        // SAFETY : un raccourci et une file de messages propres à ce fil.
        if let Err(e) = unsafe { RegisterHotKey(None, 1, MOD_WIN | MOD_NOREPEAT, VK_F12.0 as u32) } {
            let _ = envoi.send(Evenement::Echec(format!("Windows+F12 est déjà pris ({e})")));
            return;
        }
        // La webcam : son absence (touche déjà prise) n'empêche pas d'arrêter.
        // SAFETY : comme ci-dessus.
        if let Err(e) = unsafe { RegisterHotKey(None, 2, MOD_WIN | MOD_NOREPEAT, VK_F11.0 as u32) } {
            let _ = envoi.send(Evenement::Echec(format!("Windows+F11 est déjà pris ({e})")));
        }
        let _ = envoi.send(Evenement::Pret);
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
            if message.message != WM_HOTKEY {
                continue;
            }
            let e = if message.wParam.0 == 2 { Evenement::Webcam } else { Evenement::Appui };
            if envoi.send(e).is_err() {
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
