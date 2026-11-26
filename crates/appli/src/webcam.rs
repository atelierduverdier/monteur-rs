//! FILMER LA WEBCAM AVEC LA PRISE (25/11/2026, Christophe : « l'incrustation d'une webcam […] en général, les gens qui
//! font des vidéos, ils aiment bien se voir »). La caméra s'enregistre à part, par ffmpeg, pendant que l'écran se
//! filme ; le montage l'incruste ensuite (noyau::montage::Webcam), où on la place et la passe en grand.
//!
//! - Sous Linux, la caméra V4L2 (`/dev/videoN`, celle d'index 0 de chaque appareil : la seconde ne donne que des
//!   métadonnées), en MJPEG 1280 × 720 à 30 i/s quand elle le sait (la C920 de l'atelier, oui) ; sous Windows, la même
//!   par DirectShow, le ffmpeg.exe livré à côté du Monteur.
//! - Le calage : chaque image reçoit l'heure murale de son arrivée (`-use_wallclock_as_timestamps`), et ffmpeg annonce
//!   au départ celle de la première (« start: … »). La vidéo écrite commence à 0 ; la prise sait l'heure de sa propre
//!   première image : la différence est le décalage du montage. Mesuré le 25/11/2026 : la C920 donne sa première
//!   image 0,37 s après le lancement.
//! - La webcam démarre en même temps que l'écran (le choix de la fenêtre et le décompte compris) et s'arrête avec la
//!   prise : `q` sur son entrée, comme au clavier, pour un fichier fini proprement.
//! - Un format refusé par la caméra : ffmpeg s'arrête aussitôt, sans image ; on réessaie avec ce qu'elle donne par
//!   défaut.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Une caméra : son nom pour les gens, et ce que ffmpeg ouvre (`/dev/video0`, ou le nom DirectShow).
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub nom: String,
    pub entree: String,
}

/// Les caméras du poste. Sous Linux, lues dans `/sys` ; sous Windows, demandées à ffmpeg.
#[cfg(not(windows))]
pub fn cameras() -> Vec<Camera> {
    let Ok(entrees) = std::fs::read_dir("/sys/class/video4linux") else { return Vec::new() };
    let lire = |p: &Path, f: &str| std::fs::read_to_string(p.join(f)).map(|t| t.trim().to_string()).unwrap_or_default();
    let mut liste: Vec<Camera> = entrees
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| lire(p, "index") == "0")
        .filter_map(|p| {
            let noeud = p.file_name()?.to_string_lossy().into_owned();
            Some(Camera { nom: lire(&p, "name"), entree: format!("/dev/{noeud}") })
        })
        .collect();
    liste.sort_by(|a, b| a.entree.cmp(&b.entree));
    liste
}

#[cfg(windows)]
pub fn cameras() -> Vec<Camera> {
    let sortie = noyau::systeme::commande("ffmpeg")
        .args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .stdin(Stdio::null())
        .output();
    sortie.map(|o| cameras_dshow(&String::from_utf8_lossy(&o.stderr))).unwrap_or_default()
}

/// Ce qui empêche la caméra de filmer sous Windows, dit pour qu'on sache quoi activer ; `None` : rien ne l'empêche (ou
/// pas Windows). Le 26/11/2026, au PC de l'atelier : la C920 reconnue, son micro marchait, mais « Accès à la caméra »
/// était coupé dans la confidentialité de Windows ; ffmpeg ne voyait plus la caméra, et le Monteur semblait ne pas la
/// trouver. Les trois interrupteurs de Paramètres › Confidentialité et sécurité › Caméra sont dans le registre
/// (`CapabilityAccessManager\ConsentStore\webcam`, valeur `Value` : « Allow » ou « Deny »).
pub fn blocage() -> Option<String> {
    #[cfg(windows)]
    {
        cause_blocage(consentement(true, "").as_deref(), consentement(false, "").as_deref(), consentement(false, r"\NonPackaged").as_deref())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Le premier interrupteur coupé, de la machine aux applications de bureau.
#[cfg_attr(not(windows), allow(dead_code))]
fn cause_blocage(machine: Option<&str>, utilisateur: Option<&str>, bureau: Option<&str>) -> Option<String> {
    let coupe = |v: Option<&str>| v.is_some_and(|v| v.eq_ignore_ascii_case("Deny"));
    let interrupteur = if coupe(machine) {
        "« Accès à la caméra »"
    } else if coupe(utilisateur) {
        "« Autoriser les applications à accéder à votre caméra »"
    } else if coupe(bureau) {
        "« Autoriser les applications de bureau à accéder à votre caméra »"
    } else {
        return None;
    };
    Some(format!("Windows bloque la caméra : dans Paramètres › Confidentialité et sécurité › Caméra, active {interrupteur}, puis relance le Monteur."))
}

/// La valeur d'un interrupteur de la caméra : `machine` pour tout l'ordinateur, sinon pour ce compte.
#[cfg(windows)]
fn consentement(machine: bool, sous_cle: &str) -> Option<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::{HSTRING, PCWSTR, w};
    let cle = HSTRING::from(format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam{sous_cle}"));
    let racine = if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let mut tampon = [0u16; 32];
    let mut octets = std::mem::size_of_val(&tampon) as u32;
    // SAFETY : la clé vit jusqu'à la fin de l'appel ; le tampon et sa taille en octets vont ensemble.
    let e = unsafe { RegGetValueW(racine, PCWSTR(cle.as_ptr()), w!("Value"), RRF_RT_REG_SZ, None, Some(tampon.as_mut_ptr().cast()), Some(&mut octets)) };
    if e != ERROR_SUCCESS {
        return None;
    }
    let n = (octets as usize / 2).min(tampon.len());
    Some(String::from_utf16_lossy(&tampon[..n]).trim_end_matches('\0').to_string())
}

/// Les caméras dans ce que ffmpeg écrit pour `-list_devices true -f dshow` : `[dshow @ …] "Nom" (video)`.
#[cfg_attr(not(windows), allow(dead_code))]
fn cameras_dshow(texte: &str) -> Vec<Camera> {
    texte
        .lines()
        .filter(|l| l.contains("(video)"))
        .filter_map(|l| {
            let nom = l.split('"').nth(1)?.to_string();
            Some(Camera { entree: nom.clone(), nom })
        })
        .collect()
}

/// L'heure murale de la première image, dans l'annonce de ffmpeg : « Duration: N/A, start: 1795583318.246285, … ».
fn lire_debut(ligne: &str) -> Option<f64> {
    let reste = ligne.split("start: ").nth(1)?;
    reste.split([',', ' ']).next()?.trim().parse().ok().filter(|t: &f64| *t > 1.0e9)
}

/// Ce que ffmpeg ouvre : d'abord en 1280 × 720 à 30 i/s (MJPEG sous Linux : la caméra compresse, le câble USB suit),
/// puis tel que la caméra le donne.
fn essais(camera: &Camera) -> Vec<Vec<String>> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mut a = Vec::new();
    if cfg!(windows) {
        let entree = format!("video={}", camera.entree);
        let commun = s(&["-f", "dshow", "-use_wallclock_as_timestamps", "1", "-rtbufsize", "256M"]);
        a.push([commun.clone(), s(&["-video_size", "1280x720", "-framerate", "30", "-i", &entree])].concat());
        a.push([commun, s(&["-i", &entree])].concat());
    } else {
        let commun = s(&["-f", "v4l2", "-use_wallclock_as_timestamps", "1"]);
        a.push([commun.clone(), s(&["-input_format", "mjpeg", "-framerate", "30", "-video_size", "1280x720", "-i", &camera.entree])].concat());
        a.push([commun, s(&["-i", &camera.entree])].concat());
    }
    a
}

#[derive(Default)]
struct Partage {
    entree: Option<ChildStdin>,
    enfant: Option<Child>,
    debut: Option<f64>,
    arret: bool,
    derniere_ligne: String,
}

/// La webcam en train de s'enregistrer.
pub struct Webcam {
    partage: Arc<Mutex<Partage>>,
    fil: Option<JoinHandle<()>>,
    partiel: PathBuf,
    pub camera: Camera,
}

impl Webcam {
    /// Commence à filmer `camera` dans `dossier`, sous un nom `.partiel` que la liste ignore.
    pub fn lancer(camera: &Camera, dossier: &Path) -> Self {
        let partiel = dossier.join(format!("capture {} webcam.partiel.mkv", noyau::maintenant().replace([':', '/'], "-")));
        let partage = Arc::new(Mutex::new(Partage::default()));
        let (p, sortie, essais) = (partage.clone(), partiel.clone(), essais(camera));
        let fil = std::thread::Builder::new().name("webcam".into()).spawn(move || filmer(&p, &sortie, &essais)).ok();
        Self { partage, fil, partiel, camera: camera.clone() }
    }

    /// La première image est arrivée.
    pub fn tourne(&self) -> bool {
        self.partage.lock().is_ok_and(|p| p.debut.is_some())
    }

    /// Arrête et rend la vidéo finie et l'heure murale de sa première image ; l'erreur sinon. La vidéo reste sous son
    /// nom `.partiel` : à l'appelant de la ranger (ou de l'effacer).
    pub fn finir(mut self) -> Result<(PathBuf, f64), String> {
        self.arreter();
        let limite = Instant::now() + Duration::from_secs(8);
        while self.fil.as_ref().is_some_and(|f| !f.is_finished()) && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.fil.as_ref().is_some_and(|f| !f.is_finished()) {
            // Sourd à « q » : arrêté pour de bon ; le Matroska écrit jusque-là se lit quand même.
            if let Ok(mut p) = self.partage.lock()
                && let Some(e) = p.enfant.as_mut()
            {
                let _ = e.kill();
            }
        }
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
        let p = self.partage.lock().map_err(|_| "webcam : état illisible".to_string())?;
        match p.debut {
            Some(d) if self.partiel.exists() => Ok((self.partiel.clone(), d)),
            _ => {
                let _ = std::fs::remove_file(&self.partiel);
                Err(match blocage() {
                    Some(b) => format!("La webcam n'a rien filmé. {b}"),
                    None => format!("La webcam n'a rien filmé ({}).", if p.derniere_ligne.is_empty() { "pas de réponse" } else { &p.derniere_ligne }),
                })
            }
        }
    }

    /// Demande la fin : « q » à ffmpeg, comme au clavier.
    fn arreter(&mut self) {
        if let Ok(mut p) = self.partage.lock() {
            p.arret = true;
            if let Some(mut e) = p.entree.take() {
                let _ = e.write_all(b"q\n");
            }
        }
    }
}

/// Le Monteur se ferme en filmant : la webcam s'arrête proprement, son fichier reste `.partiel`.
impl Drop for Webcam {
    fn drop(&mut self) {
        if self.fil.is_none() {
            return;
        }
        self.arreter();
        let limite = Instant::now() + Duration::from_secs(3);
        while self.fil.as_ref().is_some_and(|f| !f.is_finished()) && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Le fil de la webcam : chaque essai jusqu'à ce qu'une image arrive, puis jusqu'à l'arrêt.
fn filmer(partage: &Arc<Mutex<Partage>>, sortie: &Path, essais: &[Vec<String>]) {
    let encodeur = crate::enregistreur::encodeur();
    for entree in essais {
        let mut commande = noyau::systeme::commande("ffmpeg");
        commande.args(["-hide_banner", "-nostats"]).args(entree).arg("-an").args(encodeur).args(["-f", "matroska", "-y"]).arg(sortie);
        let enfant = {
            let Ok(mut p) = partage.lock() else { return };
            if p.arret {
                return;
            }
            match commande.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn() {
                Ok(mut e) => {
                    p.entree = e.stdin.take();
                    let erreurs = e.stderr.take();
                    p.enfant = Some(e);
                    erreurs
                }
                Err(e) => {
                    p.derniere_ligne = format!("ffmpeg : {e}");
                    return;
                }
            }
        };
        // La sortie d'erreur se lit jusqu'au bout : pleine, elle bloquerait ffmpeg.
        if let Some(erreurs) = enfant {
            for ligne in BufReader::new(erreurs).lines().map_while(Result::ok) {
                let Ok(mut p) = partage.lock() else { return };
                if p.debut.is_none()
                    && let Some(d) = lire_debut(&ligne)
                {
                    p.debut = Some(d);
                }
                if !ligne.trim().is_empty() {
                    p.derniere_ligne = ligne.trim().to_string();
                }
            }
        }
        let Ok(mut p) = partage.lock() else { return };
        if let Some(mut e) = p.enfant.take() {
            let _ = e.wait();
        }
        if p.debut.is_some() || p.arret {
            return;
        }
        // Rien filmé : le format suivant.
        let _ = std::fs::remove_file(sortie);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_heure_de_la_premiere_image() {
        assert_eq!(lire_debut("  Duration: N/A, start: 1795583318.246285, bitrate: N/A"), Some(1795583318.246285));
        // La ligne du flux la répète ; un « start » de fichier ordinaire (près de 0) n'est pas une heure.
        assert_eq!(lire_debut("  Stream #0:0: Video: mjpeg, 1280x720, 30 fps, start 1795583318.246285"), None);
        assert_eq!(lire_debut("  Duration: 00:00:02.00, start: 0.000000, bitrate: 600 kb/s"), None);
    }

    #[test]
    fn le_premier_interrupteur_coupe() {
        assert_eq!(cause_blocage(None, None, None), None);
        assert_eq!(cause_blocage(Some("Allow"), Some("Allow"), Some("Allow")), None);
        // Le PC de l'atelier, le 26/11/2026 : coupé pour la machine et pour le compte, permis aux applications de bureau.
        let b = cause_blocage(Some("Deny"), Some("Deny"), Some("Allow")).unwrap();
        assert!(b.contains("« Accès à la caméra »") && b.contains("Confidentialité et sécurité › Caméra"), "{b}");
        assert!(cause_blocage(Some("Allow"), Some("deny"), None).unwrap().contains("« Autoriser les applications à accéder"));
        assert!(cause_blocage(None, None, Some("Deny")).unwrap().contains("applications de bureau"));
    }

    /// Ce que dit ce poste : `cargo test -- --ignored --nocapture blocage_de_ce_poste`, sous Windows.
    #[test]
    #[ignore]
    fn blocage_de_ce_poste() {
        println!("blocage : {:?}", blocage());
    }

    #[test]
    fn les_cameras_de_directshow() {
        let texte = r#"[dshow @ 000001] "HD Pro Webcam C920" (video)
[dshow @ 000001]   Alternative name "@device_pnp_\\?\usb#vid_046d"
[dshow @ 000001] "Microphone (HD Pro Webcam C920)" (audio)
[dshow @ 000001] "OBS Virtual Camera" (video)"#;
        let noms: Vec<String> = cameras_dshow(texte).into_iter().map(|c| c.nom).collect();
        assert_eq!(noms, ["HD Pro Webcam C920", "OBS Virtual Camera"]);
    }

    /// Une fausse caméra (une mire de ffmpeg, `lavfi`), filmée 2 s comme la vraie : l'heure de sa première image
    /// arrive, la vidéo finie se lit et commence à 0.
    #[test]
    fn une_fausse_camera_de_deux_secondes() {
        let d = std::env::temp_dir().join(format!("monteur-webcam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let partage = Arc::new(Mutex::new(Partage::default()));
        let sortie = d.join("essai webcam.partiel.mkv");
        let entree: Vec<String> = ["-f", "lavfi", "-use_wallclock_as_timestamps", "1", "-re", "-i", "testsrc=size=320x240:rate=30"].iter().map(|s| s.to_string()).collect();
        let (p, s) = (partage.clone(), sortie.clone());
        let avant = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
        let fil = std::thread::spawn(move || filmer(&p, &s, &[entree]));
        let w = Webcam { partage, fil: Some(fil), partiel: sortie, camera: Camera { nom: "mire".into(), entree: String::new() } };
        std::thread::sleep(Duration::from_secs(2));
        assert!(w.tourne(), "pas d'heure de première image");
        let (video, debut) = w.finir().unwrap();
        assert!((debut - avant).abs() < 3.0, "première image à {debut}, lancée à {avant}");
        let infos = noyau::systeme::commande("ffprobe")
            .args(["-v", "error", "-show_entries", "format=start_time,duration", "-of", "csv=p=0"])
            .arg(&video)
            .output()
            .unwrap();
        let texte = String::from_utf8_lossy(&infos.stdout).to_string();
        let _ = std::fs::remove_dir_all(&d);
        let valeurs: Vec<f64> = texte.trim().split(',').filter_map(|v| v.parse().ok()).collect();
        assert_eq!(valeurs.len(), 2, "{texte}");
        assert!(valeurs[0].abs() < 0.1, "elle commence à {}", valeurs[0]);
        assert!(valeurs[1] > 1.0, "{} s filmées", valeurs[1]);
    }
}
