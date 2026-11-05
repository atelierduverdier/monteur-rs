//! Filmer l'écran sans OBS : `gpu-screen-recorder` passe par le portail de KDE, le même dialogue
//! qu'OBS pour choisir la fenêtre. La vidéo s'écrit sous un nom `.partiel` que la liste ignore,
//! puis prend le nom daté d'OBS à l'instant de sa première image : le calage des gestes en dépend.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Capture {
    enfant: Child,
    partiel: PathBuf,
    dossier: PathBuf,
    debut: Option<Instant>,
    stoppee: bool,
    fini: bool,
    derniere_ligne: Arc<Mutex<String>>,
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

impl Capture {
    /// `jeton` : où le portail garde la fenêtre choisie, pour ne pas la redemander.
    pub fn lancer(dossier: &Path, jeton: &Path) -> Result<Self, String> {
        let programme = std::env::var_os("MONTEUR2_ECRAN").unwrap_or_else(|| "gpu-screen-recorder".into());
        let partiel = dossier.join(format!("capture {}.partiel.mp4", noyau::maintenant().replace([':', '/'], "-")));
        let mut enfant = Command::new(&programme)
            .args(["-w", "portal", "-f", "30", "-cursor", "yes", "-c", "mp4", "-ac", "aac", "-ab", "192"])
            .args(["-a", "default_output|default_input"])
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
        Ok(Self { enfant, partiel, dossier: dossier.to_path_buf(), debut: None, stoppee: false, fini: false, derniere_ligne })
    }

    /// Les secondes filmées ; `None` tant que la fenêtre n'est pas choisie.
    pub fn temps(&self) -> Option<f64> {
        self.debut.map(|d| d.elapsed().as_secs_f64())
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
        if self.debut.is_none() && horodatage_du(&self.partiel).exists() {
            self.debut = Some(Instant::now());
        }
        let etat = self.enfant.try_wait().ok()??;
        Some(self.conclure(etat.success()))
    }

    fn conclure(&mut self, reussi: bool) -> Fin {
        self.fini = true;
        let ts = horodatage_du(&self.partiel);
        let premiere = std::fs::read_to_string(&ts).ok().and_then(|t| noyau::videos::lire_premiere_image(&t));
        let _ = std::fs::remove_file(&ts);
        let Some(instant) = premiere else {
            let _ = std::fs::remove_file(&self.partiel);
            // Le dialogue refermé sans choisir fait aussi sortir le programme en erreur.
            let ligne = self.derniere_ligne.lock().map(|l| l.clone()).unwrap_or_default();
            return if self.stoppee || ligne.is_empty() || ligne.contains("canceled") || ligne.contains("cancelled") {
                Fin::Annulee
            } else {
                Fin::Erreur(format!("Enregistrement impossible : {ligne}"))
            };
        };
        // Jamais par-dessus une prise : la seconde suivante si le nom est pris.
        let cible = (0..60)
            .map(|i| self.dossier.join(format!("{}.mp4", noyau::videos::nom_de_prise(instant + i as f64))))
            .find(|c| !c.exists());
        let Some(cible) = cible else {
            return Fin::Erreur(format!("Vidéo gardée sous {}", self.partiel.display()));
        };
        match std::fs::rename(&self.partiel, &cible) {
            Ok(()) if reussi => Fin::Prise(cible),
            Ok(()) => Fin::Erreur(format!("Enregistrement interrompu : vérifie {}", cible.display())),
            Err(e) => Fin::Erreur(format!("Vidéo gardée sous {} : {e}", self.partiel.display())),
        }
    }
}

/// Le Monteur se ferme en filmant : la vidéo se termine quand même.
impl Drop for Capture {
    fn drop(&mut self) {
        if self.fini {
            return;
        }
        self.arreter();
        let limite = Instant::now() + Duration::from_secs(5);
        while Instant::now() < limite {
            if let Ok(Some(e)) = self.enfant.try_wait() {
                self.conclure(e.success());
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
