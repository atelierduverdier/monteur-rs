//! Le nanoKONTROL2 de Korg (ou une autre surface MIDI en mode CC) : ses boutons font les gestes du
//! clavier, ses faders et ses boutons rotatifs règlent le zoom choisi et la voix, son bouton 8 fait
//! défiler la tête. Il est lu par le séquenceur d'ALSA dans un fil à part : ses messages arrivent
//! même quand une autre fenêtre est active, et ● arrête de filmer depuis VerdierCAM.
//!
//! L'appareil reste dans sa scène d'usine (mode CC, canal 1) et le Monteur ne lui écrit rien : ses
//! voyants restent en mode interne. `nanokontrol2.json`, à côté des fiches, change l'affectation
//! (`{ "45": "rien", "39": "chapitre" }`) ; le Monteur le lit au lancement et ne l'écrit jamais.
//! `MONTEUR2_MIDI` remplace le nom de l'appareil cherché.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use alsa::seq::{Addr, ClientIter, EvCtrl, EventType, PortCap, PortIter, PortSubscribe, PortType, Seq};
use noyau::arrondi::arrondi;
use noyau::montage::{FACTEUR_MAX, FACTEUR_MIN};

use crate::etat::{Choix, Sorte};

/// Ce qu'on cherche dans le nom des clients et des ports MIDI.
const APPAREIL: &str = "nanoKONTROL2";
/// Une image de prise (30 i/s), comme les flèches du clavier.
const IMAGE: f64 = 1.0 / 30.0;
/// Un bouton tenu se répète après ce temps.
const AVANT_REPETITION: Duration = Duration::from_millis(400);
/// Un fader immobile plus longtemps que ça : le geste suivant est un autre pas d'annulation.
const FIN_DE_GESTE: Duration = Duration::from_millis(800);
/// La navette ne déplace pas la tête plus souvent : chaque saut décode une image.
const NAVETTE_SAUT: Duration = Duration::from_millis(80);

pub enum Evenement {
    Branche(String),
    Debranche,
    Cc(u8, u8),
    Echec(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lire,
    Stop,
    Reculer,
    Avancer,
    ImagePrecedente,
    ImageSuivante,
    Debut,
    Fin,
    Coupe,
    Chapitre,
    ChapitrePrecedent,
    ChapitreSuivant,
    Filmer,
    Zoom,
    Scinder,
    Supprimer,
    Echap,
    Annuler,
    Retablir,
    Preparer,
    Voix,
    Rendre,
    Enregistrer,
    Grossissement,
    CentreX,
    CentreY,
    Bruit,
    Grave,
    Rapport,
    Seuil,
    Adoucir,
    Navette,
}

use Action::*;

/// Les noms des gestes dans `nanokontrol2.json`.
pub const NOMS: &[(&str, Action)] = &[
    ("lire", Lire),
    ("stop", Stop),
    ("reculer", Reculer),
    ("avancer", Avancer),
    ("image_precedente", ImagePrecedente),
    ("image_suivante", ImageSuivante),
    ("debut", Debut),
    ("fin", Fin),
    ("coupe", Coupe),
    ("chapitre", Chapitre),
    ("chapitre_precedent", ChapitrePrecedent),
    ("chapitre_suivant", ChapitreSuivant),
    ("filmer", Filmer),
    ("zoom", Zoom),
    ("scinder", Scinder),
    ("supprimer", Supprimer),
    ("echap", Echap),
    ("annuler", Annuler),
    ("retablir", Retablir),
    ("preparer", Preparer),
    ("voix", Voix),
    ("rendre", Rendre),
    ("enregistrer", Enregistrer),
    ("grossissement", Grossissement),
    ("centre_x", CentreX),
    ("centre_y", CentreY),
    ("bruit", Bruit),
    ("grave", Grave),
    ("rapport", Rapport),
    ("seuil", Seuil),
    ("adoucir", Adoucir),
    ("navette", Navette),
];

/// L'affectation d'usine, sur les numéros de la scène d'usine du nanoKONTROL2 : faders CC 0 à 7,
/// boutons rotatifs 16 à 23, S 32 à 39, M 48 à 55, R 64 à 71, transport 41 à 46 et 58 à 62.
const USINE: &[(u8, Action)] = &[
    (41, Lire),
    (42, Stop),
    (43, Reculer),
    (44, Avancer),
    (45, Filmer),
    (46, Coupe),
    (58, ImagePrecedente),
    (59, ImageSuivante),
    (60, Chapitre),
    (61, ChapitrePrecedent),
    (62, ChapitreSuivant),
    // Voie 1, le zoom : S le pose, M scinde, R supprime ; le fader grossit, les boutons 1 et 2
    // placent le centre.
    (32, Zoom),
    (48, Scinder),
    (64, Supprimer),
    (0, Grossissement),
    (16, CentreX),
    (17, CentreY),
    (33, Annuler),
    (49, Retablir),
    (65, Echap),
    (34, Preparer),
    (50, Voix),
    (66, Rendre),
    (35, Enregistrer),
    (51, Debut),
    (67, Fin),
    // Faders 2 à 5 et bouton 4 : la fenêtre Nettoyer la voix, quand elle est ouverte.
    (1, Bruit),
    (2, Grave),
    (3, Rapport),
    (19, Seuil),
    (4, Adoucir),
    (23, Navette),
];

/// L'affectation d'usine, changée par le texte de `nanokontrol2.json` s'il y en a un ; et ce qui
/// n'a pas pu y être lu.
pub fn affectation(texte: Option<&str>) -> (HashMap<u8, Action>, Vec<String>) {
    let mut a: HashMap<u8, Action> = USINE.iter().copied().collect();
    let mut avis = Vec::new();
    let Some(texte) = texte else { return (a, avis) };
    let objet = match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(texte) {
        Ok(o) => o,
        Err(e) => {
            avis.push(format!("illisible ({e}), l'affectation d'usine reste"));
            return (a, avis);
        }
    };
    for (cle, valeur) in objet {
        let Some(cc) = cle.trim().parse::<u8>().ok().filter(|c| *c < 128) else {
            avis.push(format!("« {cle} » n'est pas un numéro de CC (0 à 127)"));
            continue;
        };
        match valeur.as_str() {
            Some("" | "rien") => {
                a.remove(&cc);
            }
            Some(nom) => match NOMS.iter().find(|(n, _)| *n == nom) {
                Some(&(_, action)) => {
                    a.insert(cc, action);
                }
                None => avis.push(format!("geste inconnu « {nom} » (CC {cc})")),
            },
            None => avis.push(format!("CC {cc} : le geste s'écrit entre guillemets")),
        }
    }
    (a, avis)
}

/// Comment les 128 crans d'un fader couvrent un réglage.
#[derive(Debug, Clone, Copy)]
pub enum Echelle {
    Lineaire(f64, f64),
    /// Le grossissement : chaque cran le multiplie d'autant, fin en bas comme en haut.
    Log(f64, f64),
}

impl Echelle {
    pub fn valeur(self, v: u8) -> f64 {
        let x = f64::from(v.min(127)) / 127.0;
        match self {
            Echelle::Lineaire(a, b) => a + (b - a) * x,
            Echelle::Log(a, b) => a * (b / a).powf(x),
        }
    }

    /// Le cran (0 à 127, fractionnaire) qui donnerait ce réglage.
    pub fn cran(self, y: f64) -> f64 {
        let x = match self {
            Echelle::Lineaire(a, b) => (y - a) / (b - a),
            Echelle::Log(a, b) => (y / a).ln() / (b / a).ln(),
        };
        x.clamp(0.0, 1.0) * 127.0
    }
}

/// Les gestes continus : leur échelle et leurs décimales.
fn echelle(a: Action) -> Option<(Echelle, usize)> {
    Some(match a {
        Grossissement => (Echelle::Log(FACTEUR_MIN, FACTEUR_MAX), 2),
        CentreX | CentreY => (Echelle::Lineaire(0.0, 1.0), 4),
        Bruit | Adoucir => (Echelle::Lineaire(0.0, 1.0), 2),
        Grave => (Echelle::Lineaire(40.0, 200.0), 0),
        Rapport => (Echelle::Lineaire(1.5, 8.0), 1),
        Seuil => (Echelle::Lineaire(-40.0, -6.0), 0),
        _ => return None,
    })
}

fn nom_du_reglage(a: Action) -> &'static str {
    match a {
        Grossissement => "le grossissement",
        CentreX => "le centre (gauche-droite)",
        CentreY => "le centre (haut-bas)",
        Bruit => "la réduction du bruit",
        Grave => "la coupe du grave",
        Rapport => "le rapport d'égalisation",
        Seuil => "le seuil d'égalisation",
        Adoucir => "l'adoucissement des « s »",
        _ => "ce réglage",
    }
}

/// La reprise douce : un fader qui n'est pas à la position du réglage ne le fait pas sauter, il le
/// prend en passant par sa valeur (à 3 crans près) ou en la franchissant.
pub fn reprend(precedent: Option<u8>, v: u8, cible: f64) -> bool {
    let ecart = f64::from(v) - cible;
    ecart.abs() <= 3.0 || precedent.is_some_and(|p| (f64::from(p) - cible) * ecart <= 0.0)
}

/// La navette : au centre, rien ; tournée, la tête file de 0,5 à 16 s par seconde, en arrière à
/// gauche, en avant à droite.
pub fn vitesse_navette(v: u8) -> f64 {
    const MORT: f64 = 6.0;
    let d = f64::from(v.min(127)) - 64.0;
    if d.abs() <= MORT {
        return 0.0;
    }
    let x = ((d.abs() - MORT) / (63.0 - MORT)).min(1.0);
    d.signum() * 0.5 * 32f64.powf(x)
}

/// Le chapitre d'avant : une demi-seconde après un chapitre, c'est celui d'avant lui ; le début s'il
/// n'y en a pas.
pub fn chapitre_precedent(chapitres: impl Iterator<Item = f64>, t: f64) -> f64 {
    chapitres.filter(|&c| c < t - 0.5).fold(0.0, f64::max)
}

/// Le chapitre d'après ; la fin s'il n'y en a pas.
pub fn chapitre_suivant(chapitres: impl Iterator<Item = f64>, t: f64, duree: f64) -> f64 {
    chapitres.filter(|&c| c > t + 0.05).fold(duree, f64::min)
}

/// Tenus, ces boutons se répètent à ce rythme.
fn repetition(a: Action) -> Option<Duration> {
    match a {
        ImagePrecedente | ImageSuivante => Some(Duration::from_millis(50)),
        Reculer | Avancer => Some(Duration::from_millis(250)),
        _ => None,
    }
}

// --- Le fil qui écoute ALSA ---

fn relier(seq: &Seq, de: Addr, vers: Addr) -> alsa::Result<()> {
    let s = PortSubscribe::empty()?;
    s.set_sender(de);
    s.set_dest(vers);
    seq.subscribe_port(&s)
}

/// Le premier port qu'on peut écouter dont le client ou le port porte ce nom, hors du Monteur.
fn trouver(seq: &Seq, appareil: &str, moi: i32) -> Option<(Addr, String)> {
    let voulu = appareil.to_lowercase();
    for client in ClientIter::new(seq) {
        if client.get_client() == moi {
            continue;
        }
        let nom = client.get_name().unwrap_or_default().to_string();
        for port in PortIter::new(seq, client.get_client()) {
            let lisible = port.get_capability().contains(PortCap::READ | PortCap::SUBS_READ);
            let porte_le_nom = nom.to_lowercase().contains(&voulu) || port.get_name().unwrap_or_default().to_lowercase().contains(&voulu);
            if lisible && porte_le_nom {
                return Some((port.addr(), nom));
            }
        }
    }
    None
}

fn ecouter(envoi: &Sender<Evenement>, appareil: &str) -> Result<(), String> {
    let texte = |e: alsa::Error| e.to_string();
    let seq = Seq::open(None, Some(alsa::Direction::Capture), false).map_err(texte)?;
    seq.set_client_name(c"Monteur 2").map_err(texte)?;
    // Un port où l'on peut aussi écrire d'ailleurs (`aseqsend -p "Monteur 2"`) : il sert aux essais.
    let port = seq.create_simple_port(c"entrée", PortCap::WRITE | PortCap::SUBS_WRITE, PortType::MIDI_GENERIC | PortType::APPLICATION).map_err(texte)?;
    let moi = Addr { client: seq.client_id().map_err(texte)?, port };
    // Les ports qui naissent et qui partent : l'appareil se branche et se débranche à chaud.
    relier(&seq, Addr::system_announce(), moi).map_err(texte)?;
    let mut relie = None;
    let chercher = |relie: &mut Option<Addr>| {
        if let Some((a, nom)) = trouver(&seq, appareil, moi.client)
            && relier(&seq, a, moi).is_ok()
        {
            *relie = Some(a);
            return envoi.send(Evenement::Branche(nom)).is_ok();
        }
        true
    };
    chercher(&mut relie);
    let mut entree = seq.input();
    loop {
        let (genre, ctrl, adresse) = {
            let ev = entree.event_input().map_err(texte)?;
            (ev.get_type(), ev.get_data::<EvCtrl>(), ev.get_data::<Addr>())
        };
        let suite = match genre {
            EventType::Controller => match ctrl {
                Some(c) => envoi.send(Evenement::Cc(c.param.min(127) as u8, c.value.clamp(0, 127) as u8)).is_ok(),
                None => true,
            },
            EventType::PortStart if relie.is_none() => chercher(&mut relie),
            EventType::PortExit if adresse.is_some() && adresse == relie => {
                relie = None;
                envoi.send(Evenement::Debranche).is_ok()
            }
            _ => true,
        };
        // Le Monteur s'est fermé.
        if !suite {
            return Ok(());
        }
    }
}

// --- Ce que la fenêtre en garde ---

pub struct Midi {
    recu: Receiver<Evenement>,
    /// Le nom de l'appareil relié, s'il l'est.
    pub branche: Option<String>,
    /// Il a déjà été branché : un rebranchement se dit dans la barre d'état.
    vu: bool,
    affectation: HashMap<u8, Action>,
    /// Ce que `nanokontrol2.json` avait d'illisible.
    pub avis: Vec<String>,
    /// Le dernier cran reçu de chaque contrôle, et la valeur qu'il a donnée au réglage.
    precedent: [Option<u8>; 128],
    donne: [Option<f64>; 128],
    /// Le fader qu'on bouge et son dernier cran : tout le geste est un seul pas d'annulation.
    geste: Option<(u8, Instant)>,
    /// Les boutons tenus qui se répètent : leur geste et leur prochain coup.
    tenus: HashMap<u8, (Action, Instant)>,
    /// La navette : sa vitesse (s par seconde), où elle mène la tête, quand elle peut la déplacer.
    navette: f64,
    navette_cible: Option<f64>,
    navette_saut: Instant,
    dernier_tour: Instant,
}

impl Midi {
    pub fn lancer(chemin_affectation: &Path) -> Self {
        let texte = std::fs::read_to_string(chemin_affectation).ok();
        let (affectation, avis) = affectation(texte.as_deref());
        let appareil = std::env::var("MONTEUR2_MIDI").unwrap_or_else(|_| APPAREIL.into());
        let (envoi, recu) = mpsc::channel();
        let lance = std::thread::Builder::new().name("midi".into()).spawn(move || {
            if let Err(e) = ecouter(&envoi, &appareil) {
                let _ = envoi.send(Evenement::Echec(e));
            }
        });
        if let Err(e) = lance {
            eprintln!("surface MIDI : fil non lancé : {e}");
        }
        Self {
            recu,
            branche: None,
            vu: false,
            affectation,
            avis,
            precedent: [None; 128],
            donne: [None; 128],
            geste: None,
            tenus: HashMap::new(),
            navette: 0.0,
            navette_cible: None,
            navette_saut: Instant::now(),
            dernier_tour: Instant::now(),
        }
    }

    fn arreter_la_navette(&mut self) {
        self.navette = 0.0;
        self.navette_cible = None;
    }
}

impl crate::Monteur {
    /// À chaque tour : les messages de la surface, puis les boutons tenus et la navette.
    pub(crate) fn suivre_midi(&mut self) {
        while let Ok(e) = self.midi.recu.try_recv() {
            match e {
                Evenement::Branche(nom) => {
                    eprintln!("surface MIDI reliée : {nom}");
                    if self.midi.vu {
                        self.message = format!("{nom} rebranché.");
                    }
                    self.midi.vu = true;
                    self.midi.branche = Some(nom);
                }
                Evenement::Debranche => {
                    if let Some(nom) = self.midi.branche.take() {
                        self.message = format!("{nom} débranché.");
                    }
                    self.midi.tenus.clear();
                    self.midi.arreter_la_navette();
                }
                Evenement::Echec(e) => eprintln!("surface MIDI indisponible : {e}"),
                Evenement::Cc(cc, v) => self.midi_cc(cc, v),
            }
        }
        let maintenant = Instant::now();
        let dus: Vec<(u8, Action)> = self.midi.tenus.iter().filter(|(_, (_, t))| maintenant >= *t).map(|(&cc, &(a, _))| (cc, a)).collect();
        for (cc, a) in dus {
            if let Some(r) = repetition(a) {
                self.midi.tenus.insert(cc, (a, maintenant + r));
            }
            self.midi_bouton(a);
        }
        self.suivre_la_navette(maintenant);
    }

    /// Un message de contrôle : un bouton (127 appuyé, 0 relâché), un fader ou un bouton rotatif.
    pub(crate) fn midi_cc(&mut self, cc: u8, v: u8) {
        let Some(&a) = self.midi.affectation.get(&cc) else { return };
        if a == Navette {
            self.midi.navette = vitesse_navette(v);
            if self.midi.navette == 0.0 {
                self.midi.navette_cible = None;
            }
            return;
        }
        if let Some((e, decimales)) = echelle(a) {
            self.midi_regler(cc, a, e, decimales, v);
            self.midi.precedent[cc as usize] = Some(v);
            return;
        }
        if v == 0 {
            self.midi.tenus.remove(&cc);
            return;
        }
        if repetition(a).is_some() {
            self.midi.tenus.insert(cc, (a, Instant::now() + AVANT_REPETITION));
        }
        self.midi_bouton(a);
    }

    /// La valeur actuelle d'un réglage continu, ou pourquoi il ne se règle pas maintenant.
    fn reglage_midi(&self, a: Action) -> Result<f64, &'static str> {
        let voix = &self.voix.reglages;
        match a {
            Grossissement | CentreX | CentreY => {
                let Some(p) = &self.prise else { return Err("Ouvre une prise pour régler un zoom.") };
                let Some(Choix { sorte: Sorte::Zoom, indice }) = p.choix else { return Err("Choisis un zoom pour le régler à la voie 1.") };
                let Some(z) = p.montage.zooms.get(indice) else { return Err("Choisis un zoom pour le régler à la voie 1.") };
                Ok(match a {
                    Grossissement => z.facteur,
                    CentreX => z.cx,
                    _ => z.cy,
                })
            }
            _ if !self.voix.ouverte => Err("Ouvre Nettoyer la voix (M3) pour la régler aux faders."),
            Bruit => Ok(voix.bruit_force),
            Grave => Ok(voix.grave_hz),
            Rapport => Ok(voix.rapport),
            Seuil => Ok(voix.seuil_db),
            _ => Ok(voix.adoucir_force),
        }
    }

    fn midi_regler(&mut self, cc: u8, a: Action, e: Echelle, decimales: usize, v: u8) {
        let i = usize::from(cc);
        let actuel = match self.reglage_midi(a) {
            Ok(x) => x,
            Err(pourquoi) => {
                self.message = pourquoi.into();
                return;
            }
        };
        // Le fader tient le réglage s'il est le dernier à l'avoir donné et que rien n'y a touché depuis.
        let tient = self.midi.donne[i].is_some_and(|d| (d - actuel).abs() < 1e-9);
        if !tient && !reprend(self.midi.precedent[i], v, e.cran(actuel)) {
            self.midi.donne[i] = None;
            self.message = format!("Pour reprendre {}, passe par {:.0} %.", nom_du_reglage(a), e.cran(actuel) / 1.27);
            return;
        }
        let x = arrondi(e.valeur(v), decimales);
        let nouveau_geste = !self.midi.geste.is_some_and(|(c, t)| c == cc && t.elapsed() < FIN_DE_GESTE);
        self.midi.geste = Some((cc, Instant::now()));
        let donne = match a {
            Grossissement | CentreX | CentreY => {
                let Some(p) = &mut self.prise else { return };
                let Some(Choix { indice, .. }) = p.choix else { return };
                if nouveau_geste {
                    p.retenir();
                }
                let z = &mut p.montage.zooms[indice];
                let donne = match a {
                    Grossissement => {
                        z.facteur = x;
                        z.cx = crate::etat::centre_borne(z.cx, x);
                        z.cy = crate::etat::centre_borne(z.cy, x);
                        z.facteur
                    }
                    CentreX => {
                        z.cx = crate::etat::centre_borne(x, z.facteur);
                        z.cx
                    }
                    _ => {
                        z.cy = crate::etat::centre_borne(x, z.facteur);
                        z.cy
                    }
                };
                p.modifie();
                donne
            }
            _ => {
                self.voix.regler(|r| match a {
                    Bruit => r.bruit_force = x,
                    Grave => r.grave_hz = x,
                    Rapport => r.rapport = x,
                    Seuil => r.seuil_db = x,
                    _ => r.adoucir_force = x,
                });
                x
            }
        };
        self.midi.donne[i] = Some(donne);
    }

    /// Le geste d'un bouton, comme sa touche au clavier.
    fn midi_bouton(&mut self, a: Action) {
        match a {
            Filmer => {
                if let Some(c) = &mut self.capture {
                    c.arreter();
                } else if self.capture_prevue.is_none() {
                    self.filmer_ecran();
                }
                return;
            }
            Voix => return self.voix.ouvrir(),
            Rendre => {
                self.fenetre_rendre = true;
                return;
            }
            Enregistrer => return self.enregistrer(),
            Lire | Stop => self.midi.arreter_la_navette(),
            _ => {}
        }
        let Some(p) = &mut self.prise else { return };
        let t = p.lecteur.position();
        if let Some(ic) = &mut p.intercalee {
            // Une vidéo intercalée passe : ▶ la met en pause, ■ revient à la prise.
            match a {
                Lire => ic.lecteur.basculer(),
                Stop | Echap => p.sortir_de_l_intercalee(),
                _ => {}
            }
            return;
        }
        match a {
            Lire => p.lecteur.basculer(),
            Stop => p.lecteur.stop(),
            Reculer => p.lecteur.aller_a(t - 5.0),
            Avancer => p.lecteur.aller_a(t + 5.0),
            ImagePrecedente => p.lecteur.aller_a(t - IMAGE),
            ImageSuivante => p.lecteur.aller_a(t + IMAGE),
            Debut => p.lecteur.aller_a(0.0),
            Fin => p.lecteur.aller_a(p.montage.duree),
            Coupe => self.message = p.marquer_coupe(t),
            Chapitre => {
                p.poser_chapitre(t);
                self.message = "Chapitre posé à la tête : donne-lui son titre dans l'inspecteur.".into();
            }
            ChapitrePrecedent => {
                let c = chapitre_precedent(p.montage.chapitres.iter().map(|c| c.t), t);
                p.lecteur.aller_a(c);
            }
            ChapitreSuivant => {
                let c = chapitre_suivant(p.montage.chapitres.iter().map(|c| c.t), t, p.montage.duree);
                p.lecteur.aller_a(c);
            }
            Zoom => {
                p.nouveau_zoom(t);
                self.message = "Zoom posé : trace son cadre dans la visionneuse, ou règle-le à la voie 1.".into();
            }
            Scinder => self.message = p.scinder(t),
            Supprimer => p.supprimer_choix(),
            Echap => {
                p.coupe_commencee = None;
                p.choix = None;
            }
            Annuler => p.defaire(),
            Retablir => p.refaire(),
            Preparer => p.preparer(),
            _ => {}
        }
    }

    /// La navette tournée : la tête file, la lecture arrêtée, une image tous les 80 ms.
    fn suivre_la_navette(&mut self, maintenant: Instant) {
        let dt = maintenant.duration_since(self.midi.dernier_tour).as_secs_f64().min(0.1);
        self.midi.dernier_tour = maintenant;
        let v = self.midi.navette;
        let Some(p) = self.prise.as_mut().filter(|p| p.intercalee.is_none() && v != 0.0) else {
            self.midi.navette_cible = None;
            return;
        };
        let cible = self.midi.navette_cible.get_or_insert_with(|| {
            p.lecteur.arreter();
            p.lecteur.position()
        });
        *cible = (*cible + v * dt).clamp(0.0, p.montage.duree);
        if maintenant >= self.midi.navette_saut && (*cible - p.lecteur.position()).abs() > 1e-3 {
            p.lecteur.aller_a(*cible);
            self.midi.navette_saut = maintenant + NAVETTE_SAUT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affectation_d_usine() {
        let (a, avis) = affectation(None);
        assert!(avis.is_empty());
        assert_eq!(a.get(&41), Some(&Lire));
        assert_eq!(a.get(&45), Some(&Filmer));
        assert_eq!(a.get(&0), Some(&Grossissement));
        assert_eq!(a.get(&23), Some(&Navette));
        // Chaque geste d'usine a un nom pour le fichier, et chaque nom un seul geste.
        for (_, g) in USINE {
            assert!(NOMS.iter().any(|(_, n)| n == g), "{g:?} sans nom");
        }
        for (i, (n, _)) in NOMS.iter().enumerate() {
            assert!(!NOMS[i + 1..].iter().any(|(m, _)| m == n), "{n} en double");
        }
    }

    #[test]
    fn affectation_changee_par_le_fichier() {
        let (a, avis) = affectation(Some(r#"{ "45": "rien", "39": "chapitre", "41": "stop", "200": "lire", "40": "voler", "38": 3 }"#));
        assert_eq!(a.get(&45), None);
        assert_eq!(a.get(&39), Some(&Chapitre));
        assert_eq!(a.get(&41), Some(&Stop));
        assert_eq!(a.get(&40), None);
        assert_eq!(a.get(&42), Some(&Stop), "le reste garde l'usine");
        assert_eq!(avis.len(), 3, "{avis:?}");
    }

    #[test]
    fn fichier_illisible_garde_l_usine() {
        let (a, avis) = affectation(Some("{ pas du json"));
        assert_eq!(avis.len(), 1);
        assert_eq!(a.len(), USINE.len());
    }

    #[test]
    fn echelles_aux_bouts_et_retour() {
        let g = Echelle::Log(FACTEUR_MIN, FACTEUR_MAX);
        assert!((g.valeur(0) - FACTEUR_MIN).abs() < 1e-12);
        assert!((g.valeur(127) - FACTEUR_MAX).abs() < 1e-12);
        let l = Echelle::Lineaire(40.0, 200.0);
        assert_eq!(l.valeur(0), 40.0);
        assert_eq!(l.valeur(127), 200.0);
        for v in [0u8, 1, 50, 64, 126, 127] {
            assert!((g.cran(g.valeur(v)) - f64::from(v)).abs() < 1e-9);
            assert!((l.cran(l.valeur(v)) - f64::from(v)).abs() < 1e-9);
        }
        // Hors des bornes, le cran reste sur le fader.
        assert_eq!(l.cran(500.0), 127.0);
        assert_eq!(g.cran(1.0), 0.0);
    }

    #[test]
    fn reprise_douce() {
        // Le réglage est au cran 64 : le fader en bas ne le prend pas…
        assert!(!reprend(None, 10, 64.0));
        assert!(!reprend(Some(10), 30, 64.0));
        // … il le prend en arrivant près de lui, ou en le franchissant d'un coup.
        assert!(reprend(Some(30), 62, 64.0));
        assert!(reprend(Some(60), 70, 64.0));
        assert!(reprend(Some(100), 50, 64.0));
        assert!(reprend(None, 66, 64.0));
    }

    #[test]
    fn navette() {
        assert_eq!(vitesse_navette(64), 0.0);
        assert_eq!(vitesse_navette(70), 0.0);
        assert_eq!(vitesse_navette(58), 0.0);
        assert!((vitesse_navette(127) - 16.0).abs() < 1e-9);
        assert!((vitesse_navette(0) + 16.0).abs() < 1e-9);
        assert!(vitesse_navette(71) > 0.49 && vitesse_navette(71) < 0.6);
        assert!(vitesse_navette(100) < vitesse_navette(110));
    }

    #[test]
    fn chapitres_voisins() {
        let c = [10.0, 30.0, 60.0];
        assert_eq!(chapitre_precedent(c.into_iter(), 45.0), 30.0);
        // Juste après un chapitre, on va à celui d'avant.
        assert_eq!(chapitre_precedent(c.into_iter(), 30.2), 10.0);
        assert_eq!(chapitre_precedent(c.into_iter(), 5.0), 0.0);
        assert_eq!(chapitre_suivant(c.into_iter(), 30.0, 90.0), 60.0);
        assert_eq!(chapitre_suivant(c.into_iter(), 61.0, 90.0), 90.0);
        assert_eq!(chapitre_suivant(std::iter::empty(), 5.0, 90.0), 90.0);
    }
}
