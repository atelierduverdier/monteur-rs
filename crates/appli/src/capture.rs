//! Filmer l'écran sans OBS : `gpu-screen-recorder` passe par le portail de KDE, le même dialogue
//! qu'OBS pour choisir la fenêtre. La vidéo s'écrit sous un nom `.partiel` que la liste ignore,
//! puis prend le nom daté d'OBS à l'instant de sa première image : le calage des gestes en dépend.
//!
//! Le micro de la Scarlett n'arrive que sur le canal gauche. Bureau et micro s'enregistrent donc sur
//! deux pistes, et ffmpeg les mêle à l'arrêt, le micro des deux côtés (`GRAPHE_SON`).
//!
//! Dès la première image, l'enregistrement se met en pause le temps de trois bips : la prise démarre
//! trois secondes après le choix de la fenêtre, sans que le décompte soit filmé.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Le décompte avant la première image gardée.
const DECOMPTE: f64 = 3.0;
/// Sous ce niveau, le micro passe pour muet : le bruit de fond de la Scarlett est sous −55 dB.
const SEUIL_MUET: f32 = -45.0;

pub struct Capture {
    enfant: Child,
    micro: Option<media::sons::Micro>,
    /// La pause du décompte : son début, et ce qu'elle a duré une fois finie.
    decompte: Option<Instant>,
    decale: f64,
    avec_decompte: bool,
    partiel: PathBuf,
    dossier: PathBuf,
    debut: Option<Instant>,
    /// Le lancement de l'enregistreur : le temps passé à attendre le choix de la fenêtre se compte d'ici.
    lancee: Instant,
    /// Les secondes filmées, figées à l'arrêt.
    pub duree: Option<f64>,
    stoppee: bool,
    /// L'arrêt demandé, et les relances déjà faites (SIGTERM, puis SIGKILL) si l'enregistreur ne répond pas.
    arret: Option<Instant>,
    relances: u8,
    fini: bool,
    derniere_ligne: Arc<Mutex<String>>,
    /// La mise au propre du son, en fond : elle dure quelques secondes sur une longue prise.
    travail: Option<(Receiver<Fin>, JoinHandle<()>)>,
}

pub enum Fin {
    /// La prise, et l'heure murale de son instant 0 (la webcam s'y cale).
    Prise(PathBuf, f64),
    Annulee,
    Erreur(String),
}

fn signal(enfant: &Child, nom: &str) {
    let _ = Command::new("kill").arg(nom).arg(enfant.id().to_string()).stdout(Stdio::null()).stderr(Stdio::null()).status();
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
    let s = noyau::systeme::commande("ffmpeg")
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
    /// `jeton` : où le portail garde la fenêtre choisie, pour ne pas la redemander. `None` : le
    /// dialogue s'ouvre. Filmer passe `None` depuis le 23/11/2026 (la fenêtre se choisit à chaque fois) ; Filmer
    /// VerdierCAM passe le sien depuis le 25/11/2026 (`portail-verdiercam`), pour filmer sans rien demander.
    /// `avec_decompte` : les trois bips avant de filmer.
    pub fn lancer(dossier: &Path, jeton: Option<&Path>, avec_decompte: bool) -> Result<Self, String> {
        let programme = std::env::var_os("MONTEUR2_ECRAN").unwrap_or_else(|| "gpu-screen-recorder".into());
        let partiel = dossier.join(format!("capture {}.partiel.mp4", noyau::maintenant().replace([':', '/'], "-")));
        let mut commande = noyau::systeme::commande(&programme);
        commande
            .args(["-w", "portal", "-f", "30", "-cursor", "yes", "-c", "mp4", "-ac", "aac", "-ab", "192"])
            .args(["-a", "default_output", "-a", "default_input"])
            .args(["-write-first-frame-ts", "yes"]);
        match jeton {
            Some(j) => commande.args(["-restore-portal-session", "yes", "-portal-session-token-filepath"]).arg(j),
            None => commande.args(["-restore-portal-session", "no"]),
        };
        let mut enfant = commande
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
        Ok(Self { enfant, partiel, dossier: dossier.to_path_buf(), micro: media::sons::Micro::lancer(SEUIL_MUET), decompte: None, decale: 0.0, avec_decompte, debut: None, lancee: Instant::now(), duree: None, stoppee: false, arret: None, relances: 0, fini: false, derniere_ligne, travail: None })
    }

    /// Les secondes passées à attendre le choix de la fenêtre dans le dialogue du portail ; `None` une fois choisie,
    /// arrêtée ou finie.
    pub fn attente_du_choix(&self) -> Option<f64> {
        let attend = self.debut.is_none() && self.decompte.is_none() && !self.stoppee && !self.fini && self.arret.is_none();
        attend.then(|| self.lancee.elapsed().as_secs_f64())
    }

    /// Les secondes filmées ; `None` tant que la fenêtre n'est pas choisie.
    pub fn temps(&self) -> Option<f64> {
        self.debut.map(|d| d.elapsed().as_secs_f64())
    }

    /// Le crête du micro, en dB.
    pub fn niveau(&self) -> Option<f32> {
        self.micro.as_ref().map(media::sons::Micro::niveau)
    }

    /// Le micro ne donne plus rien depuis 5 s.
    pub fn micro_muet(&self) -> bool {
        self.micro.as_ref().is_some_and(|m| m.muet_depuis() > Duration::from_secs(5))
    }

    /// Les secondes qu'il reste avant de filmer pour de bon.
    pub fn decompte_restant(&self) -> Option<u32> {
        if self.debut.is_some() {
            return None;
        }
        self.decompte.map(|d| (DECOMPTE - d.elapsed().as_secs_f64()).ceil().max(1.0) as u32)
    }

    /// Le programme a fini, le son se met au propre.
    pub fn en_finition(&self) -> bool {
        self.travail.is_some()
    }

    /// SIGINT : gpu-screen-recorder termine son fichier en 0,1 s environ.
    ///
    /// L'ENREGISTREUR QUI NE RÉPOND PLUS (08/11/2026, Christophe : VerdierCAM fermé aussitôt la prise lancée, « je n'ai pas
    /// possibilité d'arrêter l'enregistrement du Monteur ») : le bouton envoyait SIGINT une fois, puis plus rien — un
    /// programme resté sourd gardait « Arrêter » à l'écran pour toujours. `suivre` relance : SIGTERM après 3 s, SIGKILL
    /// après 6 s ; un second clic force tout de suite.
    pub fn arreter(&mut self) {
        if !self.stoppee {
            self.stoppee = true;
            self.arret = Some(Instant::now());
            signal(&self.enfant, "-INT");
        } else if !self.fini {
            self.relances = 2;
            let _ = self.enfant.kill();
        }
    }

    /// L'arrêt est demandé, le programme tourne encore.
    pub fn en_arret(&self) -> bool {
        self.stoppee && !self.fini
    }

    /// À appeler à chaque tour ; `Some` une seule fois, quand le programme a fini.
    pub fn suivre(&mut self) -> Option<Fin> {
        if let Some((recu, _)) = &self.travail {
            return recu.try_recv().ok();
        }
        if !self.avec_decompte && self.debut.is_none() && !self.stoppee && horodatage_du(&self.partiel).exists() {
            self.debut = Some(Instant::now());
        }
        if self.avec_decompte && self.decompte.is_none() && !self.stoppee && horodatage_du(&self.partiel).exists() {
            signal(&self.enfant, "-USR2");
            self.decompte = Some(Instant::now());
            std::thread::spawn(|| media::sons::jouer_des_bips(&[660.0, 660.0, 990.0]));
        }
        if let (None, Some(d)) = (self.debut, self.decompte)
            && d.elapsed().as_secs_f64() >= DECOMPTE
            && !self.stoppee
        {
            signal(&self.enfant, "-USR2");
            self.decale = d.elapsed().as_secs_f64();
            self.debut = Some(Instant::now());
        }
        if let Some(a) = self.arret {
            let ecoule = a.elapsed().as_secs_f64();
            if ecoule >= 3.0 && self.relances == 0 {
                self.relances = 1;
                signal(&self.enfant, "-TERM");
            } else if ecoule >= 6.0 && self.relances == 1 {
                self.relances = 2;
                let _ = self.enfant.kill();
            }
        }
        let etat = self.enfant.try_wait().ok()??;
        self.fini = true;
        self.micro = None;
        self.duree = self.temps();
        // Arrêté avant la fin du décompte : rien n'a été filmé.
        if self.debut.is_none() {
            let _ = std::fs::remove_file(horodatage_du(&self.partiel));
        }
        let (partiel, dossier, stoppee, reussi, decale) = (self.partiel.clone(), self.dossier.clone(), self.stoppee, etat.success(), self.decale);
        let ligne = self.derniere_ligne.lock().map(|l| l.clone()).unwrap_or_default();
        let (envoi, recu) = mpsc::channel();
        let fil = std::thread::spawn(move || {
            let _ = envoi.send(conclure(&partiel, &dossier, stoppee, reussi, decale, &ligne));
        });
        self.travail = Some((recu, fil));
        None
    }
}

fn conclure(partiel: &Path, dossier: &Path, stoppee: bool, reussi: bool, decale: f64, ligne: &str) -> Fin {
    let ts = horodatage_du(partiel);
    // La pause du décompte n'est pas dans la vidéo : elle commence à la fin de cette pause.
    let premiere = std::fs::read_to_string(&ts).ok().and_then(|t| noyau::videos::lire_premiere_image(&t)).map(|t| t + decale);
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
            Fin::Prise(cible, instant)
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
                if self.debut.is_none() {
                    let _ = std::fs::remove_file(horodatage_du(&self.partiel));
                }
                conclure(&self.partiel, &self.dossier, self.stoppee, e.success(), self.decale, &ligne);
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// VerdierCAM fermé pendant la prise (Christophe, 08/11/2026 : « je n'ai pas possibilité d'arrêter l'enregistrement du
    /// Monteur ») : un enregistreur qui ne répond plus à SIGINT doit quand même finir, sans quoi le bouton reste à
    /// « Arrêter » pour toujours. Un faux enregistreur qui ignore SIGINT le joue.
    #[test]
    fn un_enregistreur_sourd_finit_quand_meme() {
        let dossier = std::env::temp_dir().join(format!("monteur-sourd-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let faux = dossier.join("sourd.sh");
        std::fs::write(&faux, "#!/bin/sh\ntrap '' INT\nsleep 60\n").unwrap();
        Command::new("chmod").arg("+x").arg(&faux).status().unwrap();
        // SAFETY : ce contrôle est le seul de ce module à lire MONTEUR2_ECRAN.
        unsafe { std::env::set_var("MONTEUR2_ECRAN", &faux) };
        let mut c = Capture::lancer(&dossier, None, false).unwrap();
        std::thread::sleep(Duration::from_millis(500));   // que le faux ait posé son « trap » : sinon SIGINT l'abat
        c.arreter();
        let limite = Instant::now() + Duration::from_secs(15);
        let mut fin = None;
        while fin.is_none() && Instant::now() < limite {
            fin = c.suivre();
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&dossier);
        assert!(fin.is_some(), "l'enregistreur sourd tourne encore 15 s après « Arrêter »");
    }
}
