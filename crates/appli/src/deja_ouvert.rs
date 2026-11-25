//! LE DEUXIÈME LANCEMENT (25/11/2026). Le verrou (noyau::verrou) refusait un deuxième Monteur en l'écrivant sur la
//! sortie d'erreur, que personne ne voit quand on lance depuis le menu : Christophe, son Monteur ouvert sur un autre
//! bureau virtuel et oublié, l'a relancé, « c'est comme si c'était planté, alors qu'il est déjà ouvert ». Le deuxième
//! Monteur fait donc venir la fenêtre du premier avant de s'arrêter, ou, s'il n'y arrive pas, dit qu'elle est ouverte.
//!
//! Sous KDE (Wayland comme X11), un programme ne peut pas activer la fenêtre d'un autre : seul KWin le peut. Le
//! Monteur lui confie donc un petit script, qui cherche la fenêtre du processus noté dans le verrou, l'active
//! (KWin passe alors sur son bureau virtuel) et se décharge lui-même : c'est ce qui dit qu'il l'a trouvée. Ailleurs
//! (GNOME…), ou si KWin ne l'a pas trouvée, une notification du bureau. Sous Windows, la fenêtre se retrouve à son
//! titre ; à défaut, une boîte de message.

/// Le titre de la fenêtre du Monteur (main.rs le pose, il ne change pas) : c'est par lui que Windows la retrouve.
pub const TITRE: &str = "Monteur VerdierCAM";

const RESUME: &str = "Le Monteur VerdierCAM est déjà ouvert";
const DETAIL: &str = "Sa fenêtre est peut-être sur un autre bureau virtuel, ou réduite.";

/// `pid` : le processus noté dans le verrou, quand il se lit (sous Windows, le fichier verrouillé ne se lit pas).
#[cfg(target_os = "linux")]
pub fn signaler(pid: Option<u32>) {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        eprintln!("{RESUME}.");
        return;
    };
    rt.block_on(async {
        let Ok(bus) = ashpd::zbus::Connection::session().await else {
            eprintln!("{RESUME}.");
            return;
        };
        match par_kwin(&bus, pid).await {
            Ok(true) => eprintln!("{RESUME} : sa fenêtre est venue."),
            Ok(false) => notifier(&bus, "KWin n'a pas trouvé sa fenêtre").await,
            Err(e) => notifier(&bus, &e).await,
        }
    });
}

/// Le script de KWin : la fenêtre du processus `pid` (à défaut, celle qui porte le titre du Monteur) sort de la barre
/// des tâches et devient active, et KWin passe sur son bureau. Il ne se décharge que s'il l'a trouvée.
#[cfg(target_os = "linux")]
fn script_kwin(pid: Option<u32>, nom: &str) -> String {
    format!(
        r#"const PID = {pid};
for (const w of workspace.windowList()) {{
    if (!w.normalWindow) continue;
    if (PID > 0 ? w.pid !== PID : w.caption !== "{TITRE}") continue;
    w.minimized = false;
    workspace.activeWindow = w;
    callDBus("org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting", "unloadScript", "{nom}");
    break;
}}
"#,
        pid = pid.unwrap_or(0)
    )
}

/// `Ok(true)` : la fenêtre est venue ; `Ok(false)` : KWin répond, mais ne l'a pas trouvée ; `Err` : pas de KWin.
#[cfg(target_os = "linux")]
async fn par_kwin(bus: &ashpd::zbus::Connection, pid: Option<u32>) -> Result<bool, String> {
    const KWIN: &str = "org.kde.KWin";
    const SCRIPTS: &str = "org.kde.kwin.Scripting";
    let texte = |e: ashpd::zbus::Error| e.to_string();
    let nom = format!("monteur-verdiercam-venir-{}", std::process::id());
    let chemin = std::env::temp_dir().join(format!("{nom}.js"));
    std::fs::write(&chemin, script_kwin(pid, &nom)).map_err(|e| e.to_string())?;
    let chemin_texte = chemin.to_string_lossy().into_owned();
    let resultat = async {
        let id: i32 = bus
            .call_method(Some(KWIN), "/Scripting", Some(SCRIPTS), "loadScript", &(chemin_texte.as_str(), nom.as_str()))
            .await
            .map_err(texte)?
            .body()
            .deserialize()
            .map_err(texte)?;
        bus.call_method(Some(KWIN), format!("/Scripting/Script{id}").as_str(), Some("org.kde.kwin.Script"), "run", &())
            .await
            .map_err(texte)?;
        // KWin lit le fichier en fond, puis le script tourne : il se décharge s'il a trouvé la fenêtre.
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let charge: bool =
                bus.call_method(Some(KWIN), "/Scripting", Some(SCRIPTS), "isScriptLoaded", &(nom.as_str(),)).await.map_err(texte)?.body().deserialize().map_err(texte)?;
            if !charge {
                return Ok(true);
            }
        }
        let _ = bus.call_method(Some(KWIN), "/Scripting", Some(SCRIPTS), "unloadScript", &(nom.as_str(),)).await;
        Ok(false)
    }
    .await;
    let _ = std::fs::remove_file(&chemin);
    resultat
}

/// Une notification du bureau ; la sortie d'erreur seulement s'il n'y en a pas.
#[cfg(target_os = "linux")]
async fn notifier(bus: &ashpd::zbus::Connection, pourquoi: &str) {
    use ashpd::zvariant::Value;
    eprintln!("{RESUME} ({pourquoi}).");
    let indices: std::collections::HashMap<&str, Value> = std::collections::HashMap::new();
    let corps = (TITRE, 0u32, "fr.atelierduverdier.Monteur2", RESUME, DETAIL, Vec::<&str>::new(), indices, -1i32);
    let envoi = bus
        .call_method(Some("org.freedesktop.Notifications"), "/org/freedesktop/Notifications", Some("org.freedesktop.Notifications"), "Notify", &corps)
        .await;
    if let Err(e) = envoi {
        eprintln!("pas de notification : {e}");
    }
}

/// Windows : la fenêtre du Monteur, retrouvée à son titre, revient au premier plan ; Windows passe sur son bureau.
#[cfg(windows)]
pub fn signaler(_pid: Option<u32>) {
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, MB_ICONINFORMATION, MB_OK, MessageBoxW, SW_RESTORE, SetForegroundWindow, ShowWindow,
    };
    use windows::core::HSTRING;
    let titre = HSTRING::from(TITRE);
    // SAFETY : des appels Win32 sans pointeur gardé ; la fenêtre trouvée appartient au premier Monteur.
    unsafe {
        if let Ok(f) = FindWindowW(None, &titre)
            && !f.is_invalid()
        {
            if IsIconic(f).as_bool() {
                let _ = ShowWindow(f, SW_RESTORE);
            }
            if SetForegroundWindow(f).as_bool() {
                return;
            }
        }
        MessageBoxW(None, &HSTRING::from(format!("{RESUME}.\n\n{DETAIL}")), &titre, MB_OK | MB_ICONINFORMATION);
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn signaler(_pid: Option<u32>) {
    eprintln!("{RESUME}.");
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn le_script_cherche_le_bon_processus() {
        let s = script_kwin(Some(4242), "essai");
        assert!(s.contains("const PID = 4242;"));
        assert!(s.contains(r#""unloadScript", "essai")"#), "il se décharge sous son nom");
        // Sans numéro (verrou illisible), le titre de la fenêtre.
        assert!(script_kwin(None, "essai").contains("const PID = 0;"));
        assert!(s.contains(&format!(r#"w.caption !== "{TITRE}""#)));
    }
}
