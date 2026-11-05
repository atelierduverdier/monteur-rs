//! Le montage d'une prise (`<vidéo> montage.json`), au format 1 du Monteur 1 (`montage.py`).

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ecriture;

pub const FORMAT: i64 = 1;

/// Le zoom monte et redescend en `RAMPE` secondes, adoucies.
pub const RAMPE: f64 = 0.45;
pub const FACTEUR_MIN: f64 = 1.15;
pub const FACTEUR_MAX: f64 = 4.0;
/// Deux zooms séparés de moins de `PONT` secondes s'enchaînent en glissant de l'un à l'autre.
pub const PONT: f64 = 1.0;

/// Ce qu'un élément du montage porte en plus, que ce Monteur ne connaît pas : gardé et réécrit.
pub type Autres = Map<String, Value>;

fn milieu() -> f64 {
    0.5
}
fn vrai() -> bool {
    true
}
fn main() -> String {
    "main".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zoom {
    pub debut: f64,
    pub fin: f64,
    #[serde(default = "Zoom::facteur_defaut")]
    pub facteur: f64,
    /// Centre, en fraction de la largeur de la vidéo…
    #[serde(default = "milieu")]
    pub cx: f64,
    /// … et de sa hauteur.
    #[serde(default = "milieu")]
    pub cy: f64,
    /// « journal » : proposé d'après les gestes.
    #[serde(default = "main")]
    pub origine: String,
    /// Ce sur quoi on zoome (pour s'y retrouver).
    #[serde(default)]
    pub libelle: String,
    #[serde(flatten)]
    pub autres: Autres,
}

impl Zoom {
    fn facteur_defaut() -> f64 {
        1.8
    }

    pub fn nouveau(debut: f64, fin: f64) -> Self {
        Self {
            debut,
            fin,
            facteur: Self::facteur_defaut(),
            cx: 0.5,
            cy: 0.5,
            origine: main(),
            libelle: String::new(),
            autres: Autres::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Etiquette {
    pub debut: f64,
    pub fin: f64,
    pub texte: String,
    /// Le point montré (le réglage cliqué), en fractions.
    #[serde(default = "milieu")]
    pub x: f64,
    #[serde(default = "milieu")]
    pub y: f64,
    /// « reglage » : près du point ; « touche » : en bas à gauche ; « atelier » : en grand, en
    /// haut ; « fenetre » : le titre d'une boîte qui s'ouvre, sur elle.
    #[serde(default = "Etiquette::genre_defaut")]
    pub genre: String,
    #[serde(default = "main")]
    pub origine: String,
    /// Le réglage lui-même (x, y, l, h en fractions), mis en lumière.
    #[serde(default)]
    pub rect: Option<Vec<f64>>,
    #[serde(flatten)]
    pub autres: Autres,
}

impl Etiquette {
    fn genre_defaut() -> String {
        "reglage".into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clic {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default = "Clic::bouton_defaut")]
    pub bouton: String,
    #[serde(flatten)]
    pub autres: Autres,
}

impl Clic {
    fn bouton_defaut() -> String {
        "gauche".into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SousTitre {
    pub debut: f64,
    pub fin: f64,
    pub texte: String,
    #[serde(flatten)]
    pub autres: Autres,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coupe {
    pub debut: f64,
    pub fin: f64,
    #[serde(default = "vrai")]
    pub active: bool,
    /// « temps mort » : proposée d'après le son et les gestes.
    #[serde(default = "main")]
    pub origine: String,
    /// 0 : le passage est coupé. Plus de 1 : il est accéléré d'autant.
    #[serde(default)]
    pub vitesse: f64,
    #[serde(flatten)]
    pub autres: Autres,
}

/// Un chapitre : un instant de la prise et son titre. Le Monteur 1 ne le connaît pas : un montage
/// réenregistré par lui le perd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapitre {
    pub t: f64,
    pub titre: String,
    #[serde(default = "main")]
    pub origine: String,
    #[serde(flatten)]
    pub autres: Autres,
}

/// Une autre vidéo dans le montage (celle de l'usinage à l'atelier). Le Monteur 1 ne la connaît pas :
/// il ne la montre pas, et un montage réenregistré par lui la perd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insertion {
    /// L'instant de la prise où elle arrive.
    pub t: f64,
    /// Son fichier, relatif au dossier de la prise.
    pub video: String,
    /// La part qu'on en garde, en secondes de cette vidéo.
    pub debut: f64,
    pub fin: f64,
    /// « intercalee » : la prise s'arrête, la vidéo passe avec son propre son, la prise reprend.
    /// « dessus » : elle remplace l'image pendant que la voix de la prise continue.
    #[serde(default = "Insertion::mode_defaut")]
    pub mode: String,
    #[serde(flatten)]
    pub autres: Autres,
}

impl Insertion {
    fn mode_defaut() -> String {
        "intercalee".into()
    }

    pub fn duree(&self) -> f64 {
        (self.fin - self.debut).max(0.0)
    }

    pub fn par_dessus(&self) -> bool {
        self.mode == "dessus"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    pub montrer_clics: bool,
    pub montrer_etiquettes: bool,
    pub montrer_sous_titres: bool,
    /// Hauteur du texte, en fraction de la hauteur de l'image.
    pub taille_sous_titres: f64,
    /// Décochée : les sous-titres s'écrivent en .srt à côté de la vidéo rendue.
    pub incruster_sous_titres: bool,
    /// Secondes ajoutées aux temps du journal (calage fin).
    pub decalage_journal: f64,
    pub mettre_en_lumiere: bool,
    pub calage_auto: bool,
    /// Le son du rendu amené au volume de YouTube. Écrit seulement s'il est décoché : le fichier
    /// reste celui du Monteur 1.
    #[serde(skip_serializing_if = "est_vrai")]
    pub volume_egal: bool,
    #[serde(flatten)]
    pub autres: Autres,
}

fn est_vrai(v: &bool) -> bool {
    *v
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            montrer_clics: true,
            montrer_etiquettes: true,
            montrer_sous_titres: true,
            taille_sous_titres: 0.042,
            incruster_sous_titres: true,
            decalage_journal: 0.0,
            mettre_en_lumiere: true,
            calage_auto: true,
            volume_egal: true,
            autres: Autres::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Montage {
    pub format: i64,
    pub video: String,
    pub duree: f64,
    pub largeur: i64,
    pub hauteur: i64,
    /// Le journal de gestes repris, s'il y en a un.
    pub journal: String,
    pub zooms: Vec<Zoom>,
    pub etiquettes: Vec<Etiquette>,
    pub clics: Vec<Clic>,
    pub sous_titres: Vec<SousTitre>,
    pub coupes: Vec<Coupe>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub insertions: Vec<Insertion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chapitres: Vec<Chapitre>,
    pub reglages: Reglages,
    #[serde(flatten)]
    pub autres: Autres,
}

impl Default for Montage {
    fn default() -> Self {
        Self {
            format: FORMAT,
            video: String::new(),
            duree: 0.0,
            largeur: 0,
            hauteur: 0,
            journal: String::new(),
            zooms: Vec::new(),
            etiquettes: Vec::new(),
            clics: Vec::new(),
            sous_titres: Vec::new(),
            coupes: Vec::new(),
            insertions: Vec::new(),
            chapitres: Vec::new(),
            reglages: Reglages::default(),
            autres: Autres::new(),
        }
    }
}

/// Les éléments d'une liste ; un élément illisible (fichier retouché à la main) est sauté, comme
/// le fait le Monteur 1.
fn elements<T: DeserializeOwned>(d: &mut Map<String, Value>, nom: &str) -> Vec<T> {
    match d.remove(nom) {
        Some(Value::Array(liste)) => liste
            .into_iter()
            .filter(|e| e.is_object())
            .filter_map(|e| serde_json::from_value(e).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn nombre(v: Option<Value>) -> Option<f64> {
    match v {
        // `float(d.get("duree") or 0)` : tout ce qui est « faux » en Python vaut 0.
        None | Some(Value::Null) => Some(0.0),
        Some(Value::String(s)) if s.is_empty() => Some(0.0),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        Some(Value::Bool(b)) => Some(b as u8 as f64),
        _ => None,
    }
}

fn texte(v: Option<Value>) -> String {
    match v {
        Some(Value::String(s)) => s,
        None | Some(Value::Null) => String::new(),
        Some(autre) => autre.to_string(),
    }
}

impl Montage {
    /// Un montage d'après son JSON, ou `None` s'il n'en est pas un (`montage.depuis_dict`).
    pub fn depuis_valeur(valeur: Value) -> Option<Self> {
        let Value::Object(mut d) = valeur else { return None };
        if d.get("format").and_then(Value::as_i64) != Some(FORMAT) {
            return None;
        }
        d.remove("format");
        let duree = nombre(d.remove("duree"))?;
        let largeur = nombre(d.remove("largeur"))? as i64;
        let hauteur = nombre(d.remove("hauteur"))? as i64;
        let mut m = Montage {
            video: texte(d.remove("video")),
            duree,
            largeur,
            hauteur,
            journal: texte(d.remove("journal")),
            zooms: elements(&mut d, "zooms"),
            etiquettes: elements(&mut d, "etiquettes"),
            clics: elements(&mut d, "clics"),
            sous_titres: elements(&mut d, "sous_titres"),
            coupes: elements(&mut d, "coupes"),
            insertions: elements(&mut d, "insertions"),
            chapitres: elements(&mut d, "chapitres"),
            reglages: match d.remove("reglages") {
                Some(r @ Value::Object(_)) => serde_json::from_value(r).unwrap_or_default(),
                _ => Reglages::default(),
            },
            ..Default::default()
        };
        m.autres = d;
        m.trier();
        Some(m)
    }

    pub fn en_valeur(&self) -> Value {
        serde_json::to_value(self).expect("un montage s'écrit toujours")
    }

    /// Le montage enregistré, ou `None` s'il n'y en a pas (ou s'il est illisible).
    pub fn lire(chemin: &Path) -> Option<Self> {
        let texte = std::fs::read_to_string(chemin).ok()?;
        Self::depuis_valeur(serde_json::from_str(&texte).ok()?)
    }

    pub fn ecrire(&mut self, chemin: &Path) -> std::io::Result<()> {
        self.trier();
        ecriture::ecrire_json(chemin, "montage", self)
    }

    pub fn vide(&self) -> bool {
        self.zooms.is_empty()
            && self.etiquettes.is_empty()
            && self.clics.is_empty()
            && self.sous_titres.is_empty()
            && !self.coupes.iter().any(|c| c.active)
    }

    /// Chaque liste dans l'ordre du temps (tri stable, comme `list.sort` du Monteur 1).
    pub fn trier(&mut self) {
        let ordre = |a: f64, b: f64| a.total_cmp(&b);
        self.zooms.sort_by(|a, b| ordre(a.debut, b.debut));
        self.etiquettes.sort_by(|a, b| ordre(a.debut, b.debut));
        self.clics.sort_by(|a, b| ordre(a.t, b.t));
        self.sous_titres.sort_by(|a, b| ordre(a.debut, b.debut));
        self.coupes.sort_by(|a, b| ordre(a.debut, b.debut));
        self.insertions.sort_by(|a, b| ordre(a.t, b.t));
        self.chapitres.sort_by(|a, b| ordre(a.t, b.t));
    }

    fn borne(&self, fin: f64) -> f64 {
        if self.duree != 0.0 { self.duree.min(fin) } else { fin }
    }

    /// Les passages coupés (actifs, vitesse ≤ 1), fusionnés et bornés à la vidéo.
    pub fn coupes_actives(&self) -> Vec<(f64, f64)> {
        let mut morceaux: Vec<(f64, f64)> = self
            .coupes
            .iter()
            .filter(|c| c.active && c.fin > c.debut && c.vitesse <= 1.0)
            .map(|c| (c.debut.max(0.0), self.borne(c.fin)))
            .collect();
        morceaux.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut fusion: Vec<(f64, f64)> = Vec::new();
        for (a, b) in morceaux {
            match fusion.last_mut() {
                Some(dernier) if a <= dernier.1 => dernier.1 = dernier.1.max(b),
                _ if b > a => fusion.push((a, b)),
                _ => {}
            }
        }
        fusion
    }

    /// Les passages accélérés (début, fin, vitesse), hors des passages coupés et sans
    /// chevauchement entre eux : le premier posé gagne.
    pub fn accelerations(&self) -> Vec<(f64, f64, f64)> {
        let coupes = self.coupes_actives();
        let mut tries: Vec<&Coupe> = self.coupes.iter().collect();
        tries.sort_by(|a, b| a.debut.total_cmp(&b.debut));
        let mut res: Vec<(f64, f64, f64)> = Vec::new();
        for c in tries {
            if !(c.active && c.vitesse > 1.0 && c.fin > c.debut) {
                continue;
            }
            let (a, b) = (c.debut.max(0.0), self.borne(c.fin));
            if b > a
                && !coupes.iter().any(|&(x, y)| x < b && a < y)
                && !res.iter().any(|&(x, y, _)| x < b && a < y)
            {
                res.push((a, b, c.vitesse));
            }
        }
        res
    }

    pub fn duree_montee(&self) -> f64 {
        let gagne: f64 = self.accelerations().iter().map(|&(a, b, v)| (b - a) * (1.0 - 1.0 / v)).sum();
        let coupe: f64 = self.coupes_actives().iter().map(|&(a, b)| b - a).sum();
        let ajoute: f64 = self.insertions.iter().filter(|i| !i.par_dessus()).map(Insertion::duree).sum();
        (self.duree - coupe - gagne).max(0.0) + ajoute
    }

    /// La vidéo posée par-dessus la prise à l'instant `t`, et la seconde à en montrer.
    pub fn dessus_a(&self, t: f64) -> Option<(&Insertion, f64)> {
        self.insertions
            .iter()
            .find(|i| i.par_dessus() && i.t <= t && t < i.t + i.duree())
            .map(|i| (i, i.debut + (t - i.t)))
    }

    pub fn dans_une_coupe(&self, t: f64) -> bool {
        self.coupes_actives().iter().any(|&(a, b)| a <= t && t < b)
    }

    pub fn sous_titres_incrustes(&self) -> bool {
        self.reglages.montrer_sous_titres && self.reglages.incruster_sous_titres
    }

    /// Les sous-titres vont-ils dans un .srt plutôt que dans l'image ?
    pub fn sous_titres_a_part(&self) -> bool {
        self.reglages.montrer_sous_titres
            && !self.reglages.incruster_sous_titres
            && self.sous_titres.iter().any(|s| !s.texte.trim().is_empty())
    }

    /// 1, ou la vitesse du passage accéléré où tombe `t`.
    pub fn vitesse_a(&self, t: f64) -> f64 {
        self.accelerations()
            .iter()
            .find(|&&(a, b, _)| a <= t && t < b)
            .map_or(1.0, |&(_, _, v)| v)
    }
}

// ---------------------------------------------------------------------------
//  Le zoom à un instant
// ---------------------------------------------------------------------------

/// Un point de la courbe du zoom : instant, facteur, centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cle {
    pub t: f64,
    pub facteur: f64,
    pub cx: f64,
    pub cy: f64,
}

fn adoucir(u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Les points clés de la courbe de zoom. Chaque zoom monte de 1 à son facteur pendant `RAMPE`,
/// tient, puis redescend ; deux zooms qui se suivent de près s'enchaînent.
pub fn cles_zoom(zooms: &[Zoom]) -> Vec<Cle> {
    let mut zs: Vec<&Zoom> = zooms.iter().filter(|z| z.fin - z.debut > 0.05).collect();
    zs.sort_by(|a, b| a.debut.total_cmp(&b.debut));
    let mut cles = Vec::new();
    for (i, z) in zs.iter().enumerate() {
        let f = z.facteur.clamp(1.0, FACTEUR_MAX);
        let rampe = RAMPE.min((z.fin - z.debut) / 2.0);
        let enchaine_avant = i > 0 && z.debut - zs[i - 1].fin < PONT;
        let enchaine_apres = i + 1 < zs.len() && zs[i + 1].debut - z.fin < PONT;
        let cle = |t, facteur| Cle { t, facteur, cx: z.cx, cy: z.cy };
        if !enchaine_avant {
            cles.push(cle(z.debut, 1.0));
        }
        cles.push(cle(z.debut + rampe, f));
        cles.push(cle(z.fin - rampe, f));
        if !enchaine_apres {
            cles.push(cle(z.fin, 1.0));
        }
    }
    // Des zooms qui se chevauchent : on garde l'ordre du temps (tri stable, comme Python).
    cles.sort_by(|a, b| a.t.total_cmp(&b.t));
    cles
}

/// (facteur, cx, cy) à l'instant `t` ; facteur 1 = l'image entière.
pub fn etat_zoom(cles: &[Cle], t: f64) -> (f64, f64, f64) {
    let entiere = (1.0, 0.5, 0.5);
    let (Some(premier), Some(dernier)) = (cles.first(), cles.last()) else { return entiere };
    if t <= premier.t || t >= dernier.t {
        return entiere;
    }
    for paire in cles.windows(2) {
        let (a, b) = (paire[0], paire[1]);
        if a.t <= t && t <= b.t {
            let u = if b.t > a.t { adoucir((t - a.t) / (b.t - a.t)) } else { 1.0 };
            return (
                a.facteur + (b.facteur - a.facteur) * u,
                a.cx + (b.cx - a.cx) * u,
                a.cy + (b.cy - a.cy) * u,
            );
        }
    }
    entiere
}

/// La part de l'image source montrée, (x, y, l, h) en pixels, gardée dans l'image.
pub fn rect_source(facteur: f64, cx: f64, cy: f64, largeur: f64, hauteur: f64) -> (f64, f64, f64, f64) {
    let facteur = facteur.max(1.0);
    let (l, h) = (largeur / facteur, hauteur / facteur);
    let x = (cx * largeur - l / 2.0).max(0.0).min(largeur - l);
    let y = (cy * hauteur - h / 2.0).max(0.0).min(hauteur - h);
    (x, y, l, h)
}

/// L'instant `t` de la vidéo source dans la vidéo montée : on retire ce que le rendu retire du
/// son. Un instant retiré tombe au début de son passage.
pub fn temps_monte(retire: &[(f64, f64)], t: f64) -> f64 {
    let mut res = t;
    for &(a, b) in retire {
        if t >= b {
            res -= b - a;
        } else if t > a {
            res -= t - a;
        }
    }
    res.max(0.0)
}
