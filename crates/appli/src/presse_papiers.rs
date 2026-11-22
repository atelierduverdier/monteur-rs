//! Le presse-papiers du système : Ctrl+C et Ctrl+V dans les champs d'ImGui (dear-app ne le relie pas), et les
//! boutons « Copier » de l'onglet YouTube. Un seul `arboard::Clipboard`, gardé tant que le Monteur tourne : sous
//! Linux, c'est lui qui sert aux autres programmes ce qu'on a copié.

use std::sync::Mutex;

static PRESSE: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

fn avec<R>(f: impl FnOnce(&mut arboard::Clipboard) -> R) -> Option<R> {
    let mut garde = PRESSE.lock().ok()?;
    if garde.is_none() {
        *garde = arboard::Clipboard::new().ok();
    }
    garde.as_mut().map(f)
}

/// `false` si le système n'a pas de presse-papiers joignable.
pub fn copier(texte: &str) -> bool {
    avec(|c| c.set_text(texte.to_owned()).is_ok()).unwrap_or(false)
}

pub fn lire() -> Option<String> {
    avec(|c| c.get_text().ok()).flatten()
}

/// Le branchement pour ImGui.
pub struct PourImgui;

impl dear_imgui_rs::ClipboardBackend for PourImgui {
    fn get(&mut self) -> Option<String> {
        lire()
    }

    fn set(&mut self, valeur: &str) {
        copier(valeur);
    }
}
