//! Ce qui change d'un système à l'autre : les dossiers, les programmes qu'on lance et la façon de
//! les lancer. Sous Linux, les chemins restent ceux du Monteur 1 (`~/.local/share/monteur`,
//! `~/Vidéos`, `XDG_DATA_HOME` compris) ; sous Windows, ceux que Windows donne (AppData, le dossier
//! Vidéos de l'utilisateur).
//!
//! Tout programme se lance par [`commande`] : sous Windows, sans elle, chaque ffmpeg ouvrirait sa
//! fenêtre noire de console.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn maison() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// Le dossier Vidéos de l'utilisateur : `XDG_VIDEOS_DIR` sous Linux (`~/Vidéos` s'il n'est pas
/// dit), le dossier Vidéos sous Windows.
pub fn videos() -> PathBuf {
    dirs::video_dir().unwrap_or_else(|| maison().join("Vidéos"))
}

/// Les données des programmes : `XDG_DATA_HOME` ou `~/.local/share` sous Linux, AppData sous Windows.
pub fn dossier_donnees() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| maison().join(".local/share"))
}

/// Les fichiers du Monteur lui-même : fiches, réglages de la voix, gestes, verrou, plantages.
pub fn donnees() -> PathBuf {
    dossier_donnees().join("monteur")
}

/// Le fichier d'un programme : `ffmpeg.exe` sous Windows, `ffmpeg` ailleurs.
fn fichier_programme(nom: &OsStr) -> std::ffi::OsString {
    let mut f = nom.to_os_string();
    f.push(std::env::consts::EXE_SUFFIX);
    f
}

/// Un programme posé à côté du Monteur, comme le paquet les livrera (ffmpeg, whisper…). `None`
/// pour un chemin : il se lance tel quel.
fn a_cote_du_monteur(nom: &OsStr) -> Option<PathBuf> {
    if Path::new(nom).components().count() != 1 {
        return None;
    }
    let p = std::env::current_exe().ok()?.parent()?.join(fichier_programme(nom));
    p.is_file().then_some(p)
}

/// Où est le programme `nom` : à côté du Monteur d'abord, puis dans le PATH.
pub fn trouver_programme(nom: &str) -> Option<PathBuf> {
    let nom = OsStr::new(nom);
    if let Some(p) = a_cote_du_monteur(nom) {
        return Some(p);
    }
    let fichier = fichier_programme(nom);
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(&fichier)).find(|p| p.is_file()))
}

/// La commande qui lance `programme`, celui posé à côté du Monteur s'il y est. Sous Windows, sans
/// fenêtre de console.
pub fn commande(programme: impl AsRef<OsStr>) -> Command {
    let programme = programme.as_ref();
    #[allow(unused_mut)]
    let mut c = match a_cote_du_monteur(programme) {
        Some(p) => Command::new(p),
        None => Command::new(programme),
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Où le Monteur note le VerdierCAM choisi à la main, quand il ne l'a pas trouvé seul.
pub fn chemin_verdiercam_choisi() -> PathBuf {
    donnees().join("verdiercam")
}

/// Le programme VerdierCAM, existant ou non : `MONTEUR_VERDIERCAM`, sinon celui choisi à la main,
/// sinon à côté du Monteur, sinon (sous Linux) `~/VerdierCAM/programmes/VerdierCAM.AppImage`.
pub fn verdiercam() -> PathBuf {
    if let Some(p) = std::env::var_os("MONTEUR_VERDIERCAM") {
        return p.into();
    }
    let choisi = std::fs::read_to_string(chemin_verdiercam_choisi()).ok().map(|t| PathBuf::from(t.trim())).filter(|p| !p.as_os_str().is_empty());
    let nom = if cfg!(windows) { "VerdierCAM.exe" } else { "VerdierCAM.AppImage" };
    let a_cote = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join(nom)));
    let defaut = if cfg!(windows) { a_cote.clone().unwrap_or_else(|| PathBuf::from(nom)) } else { maison().join("VerdierCAM/programmes").join(nom) };
    choisi.into_iter().chain(a_cote).find(|p| p.is_file()).unwrap_or(defaut)
}

/// UN VERDIERCAM DÉJÀ OUVERT (25/11/2026). Filmer VerdierCAM le relançait toujours, et Christophe, qui l'a souvent
/// ouvert avec un travail dedans, repartait de zéro. `Some(vrai)` : un VerdierCAM ouvert note ses gestes dans
/// `gestes` (lancé par le Monteur, `VCAM_JOURNAL_VIDEO` le dit dans son environnement) ; `Some(faux)` : ouvert sans
/// journal ; `None` : pas de VerdierCAM. Les captures d'essai (`--capture`, sans fenêtre) ne comptent pas. Lu dans
/// `/proc`, sous Linux ; sous Windows, c'est la fenêtre de VerdierCAM qui le dit (capture_windows.rs).
#[cfg(target_os = "linux")]
pub fn verdiercam_ouvert(gestes: &Path) -> Option<bool> {
    verdiercam_ouvert_dans(Path::new("/proc"), gestes)
}

#[cfg(target_os = "linux")]
fn verdiercam_ouvert_dans(proc: &Path, gestes: &Path) -> Option<bool> {
    use std::os::unix::ffi::OsStrExt;
    let mut trouve = None;
    for entree in std::fs::read_dir(proc).ok()?.flatten() {
        if !entree.file_name().as_bytes().iter().all(u8::is_ascii_digit) {
            continue;
        }
        // Le programme de l'AppImage comme celui d'une construction : `verdiercam` (« (deleted) » s'il a été
        // reconstruit depuis). Un processus d'un autre utilisateur ne se lit pas : sauté.
        let Ok(exe) = std::fs::read_link(entree.path().join("exe")) else { continue };
        let nom = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if nom.trim_end_matches(" (deleted)") != "verdiercam" {
            continue;
        }
        let morceaux = |fichier: &str| std::fs::read(entree.path().join(fichier)).unwrap_or_default();
        if morceaux("cmdline").split(|o| *o == 0).any(|a| a == b"--capture") {
            continue;
        }
        let note = morceaux("environ")
            .split(|o| *o == 0)
            .any(|v| v.strip_prefix(b"VCAM_JOURNAL_VIDEO=").is_some_and(|d| Path::new(OsStr::from_bytes(d)) == gestes));
        trouve = Some(trouve.unwrap_or(false) || note);
    }
    trouve
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_chemin_se_lance_tel_quel() {
        assert_eq!(a_cote_du_monteur(OsStr::new("/usr/bin/ffmpeg")), None);
        assert_eq!(a_cote_du_monteur(OsStr::new("dossier/ffmpeg")), None);
        // Rien de ce nom à côté du programme de contrôle.
        assert_eq!(a_cote_du_monteur(OsStr::new("programme-qui-n-existe-pas")), None);
        assert_eq!(commande("programme-qui-n-existe-pas").get_program(), "programme-qui-n-existe-pas");
    }

    #[test]
    fn programme_trouve_dans_le_path() {
        let d = std::env::temp_dir().join(format!("monteur-systeme-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join(fichier_programme(OsStr::new("faux-ffmpeg")));
        std::fs::write(&f, "").unwrap();
        let avant = std::env::var_os("PATH").unwrap_or_default();
        let chemins: Vec<PathBuf> = std::iter::once(d.clone()).chain(std::env::split_paths(&avant)).collect();
        // SAFETY : aucun autre contrôle de ce module ne lit PATH, et celui-ci le remet.
        unsafe { std::env::set_var("PATH", std::env::join_paths(chemins).unwrap()) };
        let trouve = trouver_programme("faux-ffmpeg");
        unsafe { std::env::set_var("PATH", &avant) };
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(trouve, Some(f));
        assert_eq!(trouver_programme("programme-qui-n-existe-pas"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sous_linux_les_chemins_du_monteur_1() {
        // `XDG_DATA_HOME` posé (voir-fenetre.sh) ou non, c'est là que le Monteur 1 écrit.
        let attendu = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| maison().join(".local/share"));
        assert_eq!(donnees(), attendu.join("monteur"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn verdiercam_ouvert_lu_dans_proc() {
        let d = std::env::temp_dir().join(format!("monteur-proc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let gestes = Path::new("/maison/.local/share/monteur/gestes");
        let processus = |pid: &str, exe: &str, args: &[u8], env: &[u8]| {
            let p = d.join(pid);
            std::fs::create_dir_all(&p).unwrap();
            std::os::unix::fs::symlink(exe, p.join("exe")).unwrap();
            std::fs::write(p.join("cmdline"), args).unwrap();
            std::fs::write(p.join("environ"), env).unwrap();
        };
        processus("12", "/usr/bin/bash", b"bash\0", b"VCAM_JOURNAL_VIDEO=/maison/.local/share/monteur/gestes\0");
        processus("13", "/build/src/app/verdiercam", b"verdiercam\0--capture\0x.png\0", b"");
        std::fs::create_dir_all(d.join("self")).unwrap();
        assert_eq!(verdiercam_ouvert_dans(&d, gestes), None, "ni un autre programme, ni une capture d'essai");
        processus("14", "/tmp/.mount_Verdi/usr/bin/verdiercam", b"verdiercam\0", b"HOME=/maison\0");
        assert_eq!(verdiercam_ouvert_dans(&d, gestes), Some(false), "ouvert à la main : pas de journal");
        processus("15", "/build/src/app/verdiercam (deleted)", b"verdiercam\0", b"HOME=/maison\0VCAM_JOURNAL_VIDEO=/maison/.local/share/monteur/gestes\0");
        assert_eq!(verdiercam_ouvert_dans(&d, gestes), Some(true), "lancé par le Monteur : ses gestes sont notés");
        let _ = std::fs::remove_dir_all(&d);
    }
}
