//! Filmer l'écran sans OBS : `gpu-screen-recorder` passe par le portail de KDE, le même dialogue
//! qu'OBS pour choisir la fenêtre. La vidéo s'écrit sous un nom `.partiel` que la liste ignore,
//! puis prend le nom daté d'OBS à l'instant de sa première image : le calage des gestes en dépend.
//!
//! Le micro de la Scarlett n'arrive que sur le canal gauche. Bureau et micro s'enregistrent donc sur
//! deux pistes, et ffmpeg les mêle à l'arrêt, le micro des deux côtés (`graphe_son`).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Capture {
    enfant: Child,
    partiel: PathBuf,
    dossier: PathBuf,
    debut: Option<Instant>,
    /// Les secondes filmées, figées à l'arrêt.
    pub duree: Option<f64>,
    stoppee: bool,
    fini: bool,
    derniere_ligne: Arc<Mutex<String>>,
    /// La mise au propre du son, en fond : elle dure quelques secondes sur une longue prise.
    travail: Option<(Receiver<Fin>, JoinHandle<()>)>,
}

pub enum Fin {
    Prise(PathBuf),
    Annulee,
    Erreur(String),
}

fn horodatage_du(partiel: &Path) -> PathBuf {
    let mut nom = partiel.as_os_str().to_os_string();
    nom.push(".ts");
    PathBuf::from(nom)
}

/// Les deux canaux du micro vont des deux côtés, quelle que soit l'entrée de la carte ; le bureau
/// s'y ajoute sans être baissé.
const GRAPHE_SON: &str = "[0:a:1]pan=stereo|c0=c0+c1|c1=c0+c1[voix];[0:a:0][voix]amix=inputs=2:normalize=0:duration=longest[son]";

/// Une piste son stéréo à partir des deux : la vidéo est recopiée telle quelle.
fn meler_le_son(brut: &Path, sortie: &Path) -> Result<(), String> {
    let s = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(brut)
        .args(["-filter_complex", GRAPHE_SON, "-map", "0:v", "-map", "[son]", "-c:v", "copy", "-c:a", "aac", "-b:a", "192k"])
        .arg(sortie)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if s.status.success() {
        Ok(())
    } else {
        let _ = std::fs::remove_file(sortie);
        Err(String::from_utf8_lossy(&s.stderr).lines().last().unwrap_or("ffmpeg a échoué").to_string())
    }
}

impl Capture {
    /// `jeton` : où le portail garde la fenêtre choisie, pour ne pas la redemander.
    pub fn lancer(dossier: &Path, jeton: &Path) -> Result<Self, String> {
        let programme = std::env::var_os("MONTEUR2_ECRAN").unwrap_or_else(|| "gpu-screen-recorder".into());
        let partiel = dossier.join(format!("capture {}.partiel.mp4", noyau::maintenant().replace([':', '/'], "-")));
        let mut enfant = Command::new(&programme)
            .args(["-w", "portal", "-f", "30", "-cursor", "yes", "-c", "mp4", "-ac", "aac", "-ab", "192"])
            .args(["-a", "default_output", "-a", "default_input"])
            .args(["-restore-portal-session", "yes", "-write-first-frame-ts", "yes"])
            .arg("-portal-session-token-filepath")
            .arg(jeton)
            .arg("-o")
            .arg(&partiel)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("gpu-screen-recorder n'a pas pu être lancé : {e}"))?;
        // Le tuyau se vide en fond : plein, il bloquerait l'enregistrement.
        let derniere_ligne = Arc::new(Mutex::new(String::new()));
        if let Some(sortie) = enfant.stderr.take() {
            let ligne = derniere_ligne.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(sortie).lines().map_while(Result::ok) {
                    if !l.contains("update fps")
                        && let Ok(mut d) = ligne.lock()
                    {
                        *d = l;
                    }
                }
            });
        }
        Ok(Self { enfant, partiel, dossier: dossier.to_path_buf(), debut: None, duree: None, stoppee: false, fini: false, derniere_ligne, travail: None })
    }

    /// Les secondes filmées ; `None` tant que la fenêtre n'est pas choisie.
    pub fn temps(&self) -> Option<f64> {
        self.debut.map(|d| d.elapsed().as_secs_f64())
    }

    /// Le programme a fini, le son se met au propre.
    pub fn en_finition(&self) -> bool {
        self.travail.is_some()
    }

    /// SIGINT : gpu-screen-recorder termine son fichier en 0,1 s environ.
    pub fn arreter(&mut self) {
        if !self.stoppee {
            self.stoppee = true;
            let _ = Command::new("kill").arg("-INT").arg(self.enfant.id().to_string()).status();
        }
    }

    /// À appeler à chaque tour ; `Some` une seule fois, quand le programme a fini.
    pub fn suivre(&mut self) -> Option<Fin> {
        if let Some((recu, _)) = &self.travail {
            return recu.try_recv().ok();
        }
        if self.debut.is_none() && horodatage_du(&self.partiel).exists() {
            self.debut = Some(Instant::now());
        }
        let etat = self.enfant.try_wait().ok()??;
        self.fini = true;
        self.duree = self.temps();
        let (partiel, dossier, stoppee, reussi) = (self.partiel.clone(), self.dossier.clone(), self.stoppee, etat.success());
        let ligne = self.derniere_ligne.lock().map(|l| l.clone()).unwrap_or_default();
        let (envoi, recu) = mpsc::channel();
        let fil = std::thread::spawn(move || {
            let _ = envoi.send(conclure(&partiel, &dossier, stoppee, reussi, &ligne));
        });
        self.travail = Some((recu, fil));
        None
    }
}

fn conclure(partiel: &Path, dossier: &Path, stoppee: bool, reussi: bool, ligne: &str) -> Fin {
    let ts = horodatage_du(partiel);
    let premiere = std::fs::read_to_string(&ts).ok().and_then(|t| noyau::videos::lire_premiere_image(&t));
    let _ = std::fs::remove_file(&ts);
    let Some(instant) = premiere else {
        let _ = std::fs::remove_file(partiel);
        // Le dialogue refermé sans choisir fait aussi sortir le programme en erreur.
        return if stoppee || ligne.is_empty() || ligne.contains("canceled") || ligne.contains("cancelled") {
            Fin::Annulee
        } else {
            Fin::Erreur(format!("Enregistrement impossible : {ligne}"))
        };
    };
    // Jamais par-dessus une prise : la seconde suivante si le nom est pris.
    let cible = (0..60)
        .map(|i| dossier.join(format!("{}.mp4", noyau::videos::nom_de_prise(instant + i as f64))))
        .find(|c| !c.exists());
    let Some(cible) = cible else {
        return Fin::Erreur(format!("Vidéo gardée sous {}", partiel.display()));
    };
    if !reussi {
        return match std::fs::rename(partiel, &cible) {
            Ok(()) => Fin::Erreur(format!("Enregistrement interrompu : vérifie {}", cible.display())),
            Err(e) => Fin::Erreur(format!("Vidéo gardée sous {} : {e}", partiel.display())),
        };
    }
    let mixe = partiel.with_extension("son.partiel.mp4");
    let son = meler_le_son(partiel, &mixe);
    let source = if son.is_ok() { &mixe } else { partiel };
    if let Err(e) = std::fs::rename(source, &cible) {
        return Fin::Erreur(format!("Vidéo gardée sous {} : {e}", source.display()));
    }
    match son {
        Ok(()) => {
            let _ = std::fs::remove_file(partiel);
            Fin::Prise(cible)
        }
        // La prise n'est pas perdue : ses deux pistes y sont, le rendu n'en lira que la première.
        Err(e) => Fin::Erreur(format!("Prise enregistrée mais son non mêlé ({e}) : {}", cible.display())),
    }
}

/// Le Monteur se ferme en filmant : la vidéo se termine quand même.
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some((_, fil)) = self.travail.take() {
            let _ = fil.join();
            return;
        }
        if self.fini {
            return;
        }
        self.arreter();
        let limite = Instant::now() + Duration::from_secs(5);
        while Instant::now() < limite {
            if let Ok(Some(e)) = self.enfant.try_wait() {
                let ligne = self.derniere_ligne.lock().map(|l| l.clone()).unwrap_or_default();
                conclure(&self.partiel, &self.dossier, self.stoppee, e.success(), &ligne);
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
