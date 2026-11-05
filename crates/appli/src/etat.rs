//! Ce que la fenêtre tient : le dossier et ses prises, la prise ouverte, son montage, l'historique
//! (Ctrl+Z), l'enregistrement au fil des changements, et la préparation lancée en fond.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use noyau::fiches::Fiches;
use noyau::montage::Montage;
use noyau::videos::{self, Infos};

/// Un élément du montage, désigné par sa liste et sa place dans la liste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sorte {
    Zoom,
    Etiquette,
    Clic,
    SousTitre,
    Coupe,
    Insertion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choix {
    pub sorte: Sorte,
    pub indice: usize,
}

pub struct Prise {
    pub video: PathBuf,
    pub infos: Infos,
    pub montage: Montage,
    pub lecteur: media::Lecteur,
    /// La forme du son, calculée en fond (50 pics par seconde).
    pub pics: Vec<f32>,
    pics_recus: Option<Receiver<Vec<f32>>>,
    /// Les montages d'avant (Ctrl+Z) et ceux défaits (Ctrl+Maj+Z).
    avant: Vec<Montage>,
    apres: Vec<Montage>,
    /// Un changement pas encore écrit, et depuis quand.
    a_ecrire: Option<Instant>,
    pub choix: Option<Choix>,
    /// Le début d'une coupe posée par C, en attendant le second C.
    pub coupe_commencee: Option<f64>,
    pub preparation: Option<Preparation>,
    /// Grandit à chaque changement du montage : la visionneuse sait qu'il faut repeindre.
    pub version: u64,
    /// Une vidéo intercalée en train de passer : la prise attend.
    pub intercalee: Option<Intercalee>,
    /// La vidéo posée par-dessus la prise, ouverte : son fichier, sa lecture, sa dernière seconde.
    dessus: Option<(String, media::video::Video, f64)>,
    /// La position au tour d'avant, pour voir la tête passer sur une insertion.
    t_avant: f64,
}

pub struct Intercalee {
    pub lecteur: media::Lecteur,
    /// Où en sortir, en secondes de la vidéo intercalée.
    fin: f64,
    /// L'instant de la prise où reprendre.
    t: f64,
}

pub const PICS_PAR_S: u32 = 50;
const HISTOIRE_MAX: usize = 300;

pub fn chemin_montage(video: &Path) -> PathBuf {
    noyau::chemin_derive(video, noyau::SUFFIXE_PROJET)
}

impl Prise {
    pub fn ouvrir(video: &Path) -> Result<Self, String> {
        let sortie = std::process::Command::new("ffprobe")
            .args(videos::commande_sonder(video).into_iter().skip(1))
            .output()
            .map_err(|e| format!("ffprobe : {e}"))?;
        let infos = videos::lire_infos(&String::from_utf8_lossy(&sortie.stdout)).map_err(|e| e.0)?;
        let mut montage = Montage::lire(&chemin_montage(video)).unwrap_or_default();
        // Déplacée dans un projet, la vidéo n'est plus où le montage le disait.
        montage.video = video.to_string_lossy().into_owned();
        montage.duree = infos.duree;
        montage.largeur = infos.largeur;
        montage.hauteur = infos.hauteur;
        let lecteur = media::Lecteur::ouvrir(video, infos.duree, infos.son)?;
        let (envoi, recu) = mpsc::channel();
        let chemin = video.to_path_buf();
        std::thread::spawn(move || {
            let _ = envoi.send(media::pics_du_son(&chemin, PICS_PAR_S).unwrap_or_default());
        });
        Ok(Self {
            video: video.to_path_buf(),
            infos,
            montage,
            lecteur,
            pics: Vec::new(),
            pics_recus: Some(recu),
            avant: Vec::new(),
            apres: Vec::new(),
            a_ecrire: None,
            choix: None,
            coupe_commencee: None,
            preparation: None,
            version: 0,
            intercalee: None,
            dessus: None,
            t_avant: 0.0,
        })
    }

    /// À appeler avant chaque changement du montage : il entre dans l'historique.
    pub fn retenir(&mut self) {
        self.avant.push(self.montage.clone());
        if self.avant.len() > HISTOIRE_MAX {
            self.avant.remove(0);
        }
        self.apres.clear();
    }

    /// Le montage a changé : il s'écrira bientôt.
    pub fn modifie(&mut self) {
        self.a_ecrire.get_or_insert_with(Instant::now);
        self.version += 1;
    }

    /// Tout est-il écrit ?
    pub fn enregistre(&self) -> bool {
        self.a_ecrire.is_none()
    }

    pub fn defaire(&mut self) {
        if let Some(m) = self.avant.pop() {
            self.apres.push(std::mem::replace(&mut self.montage, m));
            self.choix = None;
            self.modifie();
        }
    }

    pub fn refaire(&mut self) {
        if let Some(m) = self.apres.pop() {
            self.avant.push(std::mem::replace(&mut self.montage, m));
            self.choix = None;
            self.modifie();
        }
    }

    pub fn peut_defaire(&self) -> bool {
        !self.avant.is_empty()
    }

    pub fn peut_refaire(&self) -> bool {
        !self.apres.is_empty()
    }

    /// Écrit le montage s'il a changé depuis une demi-seconde (ou tout de suite si `maintenant`).
    pub fn ecrire_si_besoin(&mut self, maintenant: bool) -> Option<String> {
        let depuis = self.a_ecrire?;
        if !maintenant && depuis.elapsed() < Duration::from_millis(500) {
            return None;
        }
        self.a_ecrire = None;
        self.montage.ecrire(&chemin_montage(&self.video)).err().map(|e| format!("montage non écrit : {e}"))
    }

    /// Fait avancer ce qui tourne en fond : la forme du son, la préparation.
    pub fn suivre_le_fond(&mut self) -> Option<String> {
        if let Some(r) = &self.pics_recus
            && let Ok(p) = r.try_recv()
        {
            self.pics = p;
            self.pics_recus = None;
        }
        let fini = self.preparation.as_ref().and_then(|p| p.resultat.try_recv().ok());
        if let Some(resultat) = fini {
            self.preparation = None;
            return Some(match resultat {
                Ok((m, compte)) => {
                    self.retenir();
                    self.montage = m;
                    self.choix = None;
                    self.modifie();
                    compte
                }
                Err(e) => format!("préparation impossible : {e}"),
            });
        }
        None
    }

    pub fn supprimer_choix(&mut self) {
        let Some(c) = self.choix.take() else { return };
        self.retenir();
        let m = &mut self.montage;
        match c.sorte {
            Sorte::Zoom if c.indice < m.zooms.len() => drop(m.zooms.remove(c.indice)),
            Sorte::Etiquette if c.indice < m.etiquettes.len() => drop(m.etiquettes.remove(c.indice)),
            Sorte::Clic if c.indice < m.clics.len() => drop(m.clics.remove(c.indice)),
            Sorte::SousTitre if c.indice < m.sous_titres.len() => drop(m.sous_titres.remove(c.indice)),
            Sorte::Coupe if c.indice < m.coupes.len() => drop(m.coupes.remove(c.indice)),
            Sorte::Insertion if c.indice < m.insertions.len() => drop(m.insertions.remove(c.indice)),
            _ => {}
        }
        self.modifie();
    }

    /// C : début d'une coupe, puis C : sa fin (dans l'ordre qu'on veut).
    pub fn marquer_coupe(&mut self, t: f64) -> String {
        match self.coupe_commencee.take() {
            None => {
                self.coupe_commencee = Some(t);
                "Début de la coupe posé : C à sa fin (Échap abandonne).".into()
            }
            Some(a) => {
                let (debut, fin) = (a.min(t), a.max(t));
                if fin - debut < 0.05 {
                    return "Coupe trop courte : rien posé.".into();
                }
                self.ajouter_coupe(debut, fin);
                format!("Coupe de {} à {} posée.", temps(debut), temps(fin))
            }
        }
    }

    pub fn ajouter_coupe(&mut self, debut: f64, fin: f64) {
        self.retenir();
        self.montage.coupes.push(noyau::montage::Coupe {
            debut: noyau::arrondi::arrondi(debut, 3),
            fin: noyau::arrondi::arrondi(fin, 3),
            active: true,
            origine: "main".into(),
            vitesse: 0.0,
            autres: Default::default(),
        });
        self.montage.trier();
        self.modifie();
    }

    /// Le dossier du projet : celui de la prise.
    pub fn dossier(&self) -> &Path {
        self.video.parent().unwrap_or(Path::new("."))
    }

    /// Le fichier d'une vidéo insérée (son chemin est relatif au dossier de la prise).
    pub fn chemin_insere(&self, relatif: &str) -> PathBuf {
        self.dossier().join(relatif)
    }

    /// Pose une vidéo importée à l'instant `t` ; elle se règle ensuite dans l'inspecteur.
    pub fn ajouter_insertion(&mut self, video: String, duree: f64, par_dessus: bool, t: f64) {
        self.retenir();
        let t = noyau::arrondi::arrondi(t, 3);
        let fin = if par_dessus { duree.min(self.montage.duree - t) } else { duree };
        self.montage.insertions.push(noyau::montage::Insertion {
            t,
            video: video.clone(),
            debut: 0.0,
            fin: noyau::arrondi::arrondi(fin.max(0.1), 3),
            mode: if par_dessus { "dessus" } else { "intercalee" }.into(),
            autres: Default::default(),
        });
        self.montage.trier();
        let indice = self.montage.insertions.iter().position(|i| i.t == t && i.video == video);
        self.choix = indice.map(|indice| Choix { sorte: Sorte::Insertion, indice });
        self.modifie();
    }

    /// Fait passer les vidéos intercalées : quand la tête arrive sur l'une, la prise s'arrête, la
    /// vidéo passe avec son son, puis la prise reprend. Rend un message s'il y a un souci.
    pub fn suivre_les_insertions(&mut self) -> Option<String> {
        if let Some(ic) = &mut self.intercalee {
            if ic.lecteur.position() >= ic.fin - 0.02 || ic.lecteur.position() >= ic.lecteur.duree - 0.02 {
                let t = ic.t;
                self.intercalee = None;
                self.lecteur.aller_a(t);
                self.lecteur.lire();
                self.t_avant = t;
            }
            return None;
        }
        let t = self.lecteur.position();
        let avant = std::mem::replace(&mut self.t_avant, t);
        // Un saut de la tête n'est pas un passage : seule la lecture déclenche, sur moins d'une
        // seconde.
        if !self.lecteur.en_lecture() || t < avant || t - avant > 1.0 {
            return None;
        }
        let i = self.montage.insertions.iter().find(|i| !i.par_dessus() && avant < i.t && i.t <= t)?.clone();
        let chemin = self.chemin_insere(&i.video);
        let duree = sonder_duree(&chemin).unwrap_or(i.fin);
        match media::Lecteur::ouvrir(&chemin, duree, true) {
            Ok(mut lecteur) => {
                self.lecteur.arreter();
                lecteur.aller_a(i.debut);
                lecteur.lire();
                self.intercalee = Some(Intercalee { lecteur, fin: i.fin, t: i.t });
                None
            }
            Err(e) => Some(format!("Vidéo insérée illisible : {e}")),
        }
    }

    /// Stoppe une vidéo intercalée : on reste sur la prise, au point d'insertion.
    pub fn sortir_de_l_intercalee(&mut self) {
        if let Some(ic) = self.intercalee.take() {
            self.lecteur.aller_a(ic.t);
        }
    }

    /// Ce qui remplace l'image de la prise à l'instant `t`, s'il y a quelque chose.
    pub fn image_inseree(&mut self, t: f64) -> Option<&media::Image> {
        if self.intercalee.is_some() {
            return self.intercalee.as_mut()?.lecteur.image();
        }
        let (relatif, s) = self.montage.dessus_a(t).map(|(i, s)| (i.video.clone(), s))?;
        let en_lecture = self.lecteur.en_lecture();
        if self.dessus.as_ref().is_none_or(|d| d.0 != relatif) {
            let video = media::video::Video::ouvrir(&self.chemin_insere(&relatif)).ok()?;
            self.dessus = Some((relatif, video, f64::NEG_INFINITY));
        }
        let d = self.dessus.as_mut()?;
        // En lecture, le décodage suit seul ; à l'arrêt ou après un saut, on va à la seconde voulue.
        let ecart = (s - d.2).abs();
        if ecart > 0.25 || (!en_lecture && ecart > 0.001) {
            d.1.aller_a(s);
        }
        d.2 = s;
        d.1.image_a(s)
    }

    /// Un zoom neuf de 3 s à la tête de lecture, choisi : on trace son cadre dans la visionneuse.
    pub fn nouveau_zoom(&mut self, t: f64) {
        self.retenir();
        let debut = noyau::arrondi::arrondi(t, 3);
        let fin = noyau::arrondi::arrondi((t + 3.0).min(self.montage.duree), 3);
        self.montage.zooms.push(noyau::montage::Zoom::nouveau(debut, fin));
        self.montage.trier();
        let indice = self.montage.zooms.iter().position(|z| z.debut == debut && z.fin == fin);
        self.choix = indice.map(|indice| Choix { sorte: Sorte::Zoom, indice });
        self.lecteur.arreter();
        self.modifie();
    }

    /// Lance la préparation du montage en fond (calage, gestes, temps morts).
    pub fn preparer(&mut self) {
        if self.preparation.is_none() {
            self.preparation = Some(Preparation::lancer(&self.video, self.montage.clone()));
        }
    }
}

/// La durée d'une vidéo, par ffprobe.
pub fn sonder_duree(chemin: &Path) -> Option<f64> {
    let sortie = std::process::Command::new("ffprobe").args(videos::commande_sonder(chemin).into_iter().skip(1)).output().ok()?;
    videos::lire_infos(&String::from_utf8_lossy(&sortie.stdout)).ok().map(|i| i.duree)
}

/// Le zoom d'un cadre tracé dans la visionneuse, du point fixe (ax, ay) à la souris (fx, fy), en
/// fractions de l'image : (facteur, cx, cy). Le cadre garde la forme de l'image : le plus grand des
/// deux côtés tracés, borné aux grossissements permis, et il reste dans l'image.
pub fn zoom_trace(ax: f64, ay: f64, fx: f64, fy: f64) -> (f64, f64, f64) {
    use noyau::arrondi::arrondi;
    use noyau::montage::{FACTEUR_MAX, FACTEUR_MIN};
    let (dx, dy) = (fx - ax, fy - ay);
    let cote = dx.abs().max(dy.abs()).clamp(1.0 / FACTEUR_MAX, 1.0 / FACTEUR_MIN);
    (
        arrondi(1.0 / cote, 2),
        arrondi((ax + cote / 2.0 * dx.signum()).clamp(cote / 2.0, 1.0 - cote / 2.0), 4),
        arrondi((ay + cote / 2.0 * dy.signum()).clamp(cote / 2.0, 1.0 - cote / 2.0), 4),
    )
}

/// Le centre d'un cadre déplacé : il reste dans l'image, quel que soit le grossissement tapé.
pub fn centre_borne(c: f64, facteur: f64) -> f64 {
    let demi = 0.5 / facteur.max(1.0);
    noyau::arrondi::arrondi(c.clamp(demi, 1.0 - demi), 4)
}

/// « 1:02,4 » : minutes, secondes, dixièmes.
pub fn temps(t: f64) -> String {
    let t = t.max(0.0);
    let dixiemes = (t * 10.0).round() as i64;
    let (m, reste) = (dixiemes / 600, dixiemes % 600);
    format!("{m}:{:02},{}", reste / 10, reste % 10)
}

// ---------------------------------------------------------------------------
//  La préparation : ce que « Préparer le montage » du Monteur 1 fait, dans un fil
// ---------------------------------------------------------------------------

pub struct Preparation {
    pub debut: Instant,
    resultat: Receiver<Result<(Montage, String), String>>,
}

impl Preparation {
    fn lancer(video: &Path, montage: Montage) -> Self {
        let (envoi, resultat) = mpsc::channel();
        let video = video.to_path_buf();
        std::thread::spawn(move || {
            let _ = envoi.send(preparer(&video, montage));
        });
        Self { debut: Instant::now(), resultat }
    }
}

fn lancer(commande: Vec<String>) -> Result<std::process::Output, String> {
    std::process::Command::new(&commande[0]).args(&commande[1..]).output().map_err(|e| format!("{} : {e}", commande[0]))
}

fn preparer(video: &Path, mut m: Montage) -> Result<(Montage, String), String> {
    use noyau::{analyse, journal, temps_morts};
    let gris = lancer(analyse::commande_image_grise(video))?;
    let mouvement = analyse::mouvement_image(&gris.stdout);
    let gestes = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".local/share"))
        .join("monteur/gestes");
    let dossiers = [video.parent().unwrap_or(Path::new(".")).to_path_buf(), gestes];
    let j = journal::trouver_journal(video, m.duree, &dossiers);
    let mut compte = Vec::new();
    if let Some(j) = &j {
        if !j.filme_par_verdiercam() && m.reglages.calage_auto && !mouvement.is_empty() {
            let clics = analyse::instants_actifs(j, j.decalage(video, m.duree), true);
            match analyse::calage_automatique(&mouvement, &clics, 1.5, 0.05) {
                Some(t) => {
                    m.reglages.decalage_journal = t;
                    compte.push(format!("gestes calés de {t:+.2} s").replace('.', ","));
                }
                None => compte.push("calage : rien ne ressort, gardé tel quel".into()),
            }
        }
        analyse::analyser_journal(j, &mut m, video, Some(&mouvement));
        analyse::caler_etiquettes_sur_la_voix(&mut m);
    } else {
        compte.push("pas de journal de gestes pour cette prise".into());
    }
    let silences = lancer(temps_morts::commande_silences(video))?;
    let silences = temps_morts::lire_silences(&String::from_utf8_lossy(&silences.stderr), m.duree);
    let actifs = j.as_ref().map(|j| analyse::instants_actifs(j, j.decalage(video, m.duree) + m.reglages.decalage_journal, false));
    let nouvelles = temps_morts::temps_morts(&silences, actifs.as_deref(), Some(&mouvement));
    let (coupes, acceleres) = (nouvelles.iter().filter(|c| c.vitesse <= 1.0).count(), nouvelles.iter().filter(|c| c.vitesse > 1.0).count());
    m.coupes.retain(|c| c.origine != "temps mort");
    m.coupes.extend(nouvelles);
    m.trier();
    compte.insert(
        0,
        format!(
            "Montage préparé : {} zoom(s), {} étiquette(s), {} clic(s), {coupes} temps mort(s), {acceleres} passage(s) accéléré(s) ; durée montée {} sur {}",
            m.zooms.len(),
            m.etiquettes.len(),
            m.clics.len(),
            temps(m.duree_montee()),
            temps(m.duree)
        ),
    );
    Ok((m, compte.join(" ; ") + "."))
}

// ---------------------------------------------------------------------------
//  Le dossier
// ---------------------------------------------------------------------------

pub struct Dossier {
    pub chemin: PathBuf,
    pub prises: Vec<(PathBuf, &'static str)>,
}

impl Dossier {
    pub fn lire(chemin: &Path, fiches: &Fiches) -> Self {
        let prises = videos::lister_videos(chemin)
            .into_iter()
            .map(|p| {
                let e = videos::etat(&p, fiches.fiches.get(&videos::cle(&p)));
                (p, e)
            })
            .filter(|(_, e)| *e != "ignorée")
            .collect();
        Self { chemin: chemin.to_path_buf(), prises }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadre_trace() {
        // Un quart de l'image, de haut en bas et de gauche à droite : ×2 centré sur son milieu.
        assert_eq!(zoom_trace(0.25, 0.25, 0.75, 0.75), (2.0, 0.5, 0.5));
        // Tracé à l'envers, même cadre.
        assert_eq!(zoom_trace(0.75, 0.75, 0.25, 0.25), (2.0, 0.5, 0.5));
        // Le plus grand côté décide : large et plat, le cadre garde la forme de l'image.
        let (f, cx, cy) = zoom_trace(0.1, 0.5, 0.6, 0.55);
        assert_eq!((f, cx, cy), (2.0, 0.35, 0.75));
        // Un simple clic : le plus fort grossissement, jamais plus.
        assert_eq!(zoom_trace(0.5, 0.5, 0.5, 0.5).0, 4.0);
        // Toute l'image : le plus faible, et le cadre reste dedans.
        let (f, cx, cy) = zoom_trace(0.0, 0.0, 1.0, 1.0);
        assert_eq!(f, 1.15);
        assert!((0.43..=0.57).contains(&cx) && (0.43..=0.57).contains(&cy));
    }

    #[test]
    fn centre_dans_l_image_meme_mal_regle() {
        assert_eq!(centre_borne(0.1, 2.0), 0.25);
        assert_eq!(centre_borne(0.9, 2.0), 0.75);
        // Un grossissement tapé sous 1, ou nul : ni arrêt, ni cadre hors de l'image.
        assert_eq!(centre_borne(0.9, 0.5), 0.5);
        assert_eq!(centre_borne(0.2, 0.0), 0.5);
    }
}
