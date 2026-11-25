//! UNE NOTIFICATION MUETTE (25/11/2026). Pendant qu'on filme, la fenêtre du Monteur est cachée derrière VerdierCAM :
//! le témoin « ● webcam en grand » ne se voit pas, et Christophe ne savait pas si son appui avait été pris. Un bip se
//! serait enregistré (le Monteur filme aussi le son de l'ordinateur) : c'est donc une bulle du bureau, sans son, qui
//! le dit. Une prise d'une seule fenêtre ne la filme pas ; « Tout l'écran », si.
//!
//! Sous Linux, `org.freedesktop.Notifications` (le bureau de KDE comme les autres), l'indice `suppress-sound`, et la
//! bulle d'avant remplacée plutôt qu'empilée. Sous Windows, la bulle d'une icône de la zone de notification
//! (`Shell_NotifyIconW`, `NIIF_NOSOUND`) : elle ne demande pas au Monteur d'être un paquet du Store.

/// Montre la bulle, sans attendre : elle part dans un fil à elle.
pub fn montrer(resume: &str, detail: &str) {
    let (resume, detail) = (resume.to_string(), detail.to_string());
    let _ = std::thread::Builder::new().name("notification".into()).spawn(move || envoyer(&resume, &detail));
}

#[cfg(target_os = "linux")]
static DERNIERE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(target_os = "linux")]
fn envoyer(resume: &str, detail: &str) {
    use std::sync::atomic::Ordering;

    use ashpd::zvariant::Value;
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
    rt.block_on(async {
        let Ok(bus) = ashpd::zbus::Connection::session().await else { return };
        let mut indices: std::collections::HashMap<&str, Value> = std::collections::HashMap::new();
        indices.insert("suppress-sound", Value::from(true));
        indices.insert("urgency", Value::from(0u8));
        indices.insert("desktop-entry", Value::from("fr.atelierduverdier.Monteur2"));
        let corps = (crate::deja_ouvert::TITRE, DERNIERE.load(Ordering::Relaxed), "fr.atelierduverdier.Monteur2", resume, detail, Vec::<&str>::new(), indices, 2500i32);
        let reponse = bus
            .call_method(Some("org.freedesktop.Notifications"), "/org/freedesktop/Notifications", Some("org.freedesktop.Notifications"), "Notify", &corps)
            .await;
        match reponse.map(|r| r.body().deserialize::<u32>()) {
            Ok(Ok(id)) => DERNIERE.store(id, Ordering::Relaxed),
            Ok(Err(e)) => eprintln!("notification : {e}"),
            Err(e) => eprintln!("notification : {e}"),
        }
    });
}

/// Windows : une icône dans la zone de notification, posée sur la fenêtre du Monteur, et sa bulle.
#[cfg(windows)]
fn envoyer(resume: &str, detail: &str) {
    use windows::Win32::UI::Shell::{NIF_ICON, NIF_INFO, NIF_TIP, NIIF_INFO, NIIF_NOSOUND, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IDI_INFORMATION, LoadIconW};
    use windows::core::HSTRING;
    fn copier<const N: usize>(cible: &mut [u16; N], texte: &str) {
        for (c, u) in cible.iter_mut().take(N - 1).zip(texte.encode_utf16()) {
            *c = u;
        }
    }
    // SAFETY : des appels Win32 sur des structures remplies ici ; la fenêtre trouvée est celle du Monteur.
    unsafe {
        let Ok(fenetre) = FindWindowW(None, &HSTRING::from(crate::deja_ouvert::TITRE)) else { return };
        let mut d = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: fenetre,
            uID: 1,
            uFlags: NIF_ICON | NIF_TIP | NIF_INFO,
            hIcon: LoadIconW(None, IDI_INFORMATION).unwrap_or_default(),
            dwInfoFlags: NIIF_INFO | NIIF_NOSOUND,
            ..Default::default()
        };
        copier(&mut d.szTip, crate::deja_ouvert::TITRE);
        copier(&mut d.szInfoTitle, resume);
        copier(&mut d.szInfo, detail);
        if !Shell_NotifyIconW(NIM_MODIFY, &d).as_bool() {
            let _ = Shell_NotifyIconW(NIM_ADD, &d);
        }
    }
}

/// Windows : l'icône de la zone de notification s'en va (à la fin de la prise) ; ailleurs, rien à faire.
pub fn retirer() {
    #[cfg(windows)]
    // SAFETY : comme dans `envoyer`.
    unsafe {
        use windows::Win32::UI::Shell::{NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        let Ok(fenetre) = FindWindowW(None, &windows::core::HSTRING::from(crate::deja_ouvert::TITRE)) else { return };
        let d = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: fenetre, uID: 1, ..Default::default() };
        let _ = Shell_NotifyIconW(NIM_DELETE, &d);
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn envoyer(_resume: &str, _detail: &str) {}
