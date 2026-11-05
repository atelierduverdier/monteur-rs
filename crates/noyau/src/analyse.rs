//! Ce qu'on tire du journal des gestes : zooms, étiquettes, clics ; le calage du journal sur la
//! vidéo et l'arrêt de ce qui montre un endroit quand l'écran change d'un coup. Porté à
//! l'identique de `montage.py` du Monteur 1, constantes et mesures comprises.

use std::path::Path;

use serde_json::{Map, Value};

use crate::arrondi::{arrondi, entier};
use crate::journal::Journal;
use crate::montage::{Clic, Etiquette, Montage, RAMPE, Zoom, rect_source};

/// Un zoom arrive au plus tôt `AVANT_MAX` avant le clic, au plus tard `AVANT_MIN` avant.
pub const AVANT_MIN: f64 = 0.35;
pub const AVANT_MAX: f64 = 1.5;
/// Il se défait dès que la souris s'en va ; si elle reste, il tient `APRES_MAX` après le dernier
/// geste.
pub const APRES_MIN: f64 = 0.8;
pub const APRES_MAX: f64 = 3.0;
/// La souris « sort » un peu avant le bord du cadre (fraction de sa taille).
pub const MARGE_SORTIE: f64 = 0.08;
/// Des gestes plus proches que ça (dans le temps, et sur l'écran) partagent un zoom.
pub const REGROUPER_S: f64 = 3.5;
pub const REGROUPER_ECART: f64 = 0.28;
/// La part d'image minimale qu'un zoom montre autour d'un réglage.
pub const VOISINAGE: f64 = 0.42;
/// Un zoom proposé ne grossit pas plus que ça : au-delà, on perd où l'on est.
pub const FACTEUR_PROPOSE_MAX: f64 = 2.0;
/// Une fenêtre plus petite que ça (dans les deux sens) est une boîte de dialogue : on la montre
/// entière. Un panneau haut et étroit n'en est pas une.
pub const BOITE_MAX: f64 = 0.64;
pub const DUREE_ETIQUETTE: f64 = 2.6;
pub const DUREE_TOUCHE: f64 = 1.8;
pub const DUREE_ATELIER: f64 = 1.8;
/// Tout l'écran change d'un coup : l'image bouge de plus que ça (0–255, en moyenne) d'un
/// dixième de seconde au suivant. Mesuré le 02/11/2026 : 25 à 82 quand le navigateur passe
/// devant VerdierCAM ou qu'une boîte s'ouvre ou se ferme, moins de 4 sinon.
pub const CHANGEMENT_ECRAN: f64 = 12.0;
/// Un changement à moins de ça d'un événement du journal vient de VerdierCAM lui-même.
pub const EXPLIQUE_S: f64 = 0.6;
/// Une étiquette que l'écran coupe court arrive plus tôt, pour rester lisible ce temps-là.
pub const ETIQUETTE_LUE: f64 = 1.0;
/// Un clic tenu plus longtemps, ou lâché plus loin (en pixels), est un glisser.
pub const GLISSER_S: f64 = 0.45;
pub const GLISSER_PX: f64 = 25.0;
/// L'image se juge 10 fois par seconde, en 96 × 60 gris.
pub const IMAGES_PAR_S: f64 = 10.0;
pub const GRIS_L: usize = 96;
pub const GRIS_H: usize = 60;

/// (x, y, l, h) en fractions de l'image.
pub type Rect = [f64; 4];

#[derive(Debug, Clone, PartialEq)]
pub struct Geste {
    /// En secondes de la vidéo.
    pub t: f64,
    pub x: f64,
    pub y: f64,
    /// L'élément visé.
    pub rect: Option<Rect>,
    pub libelle: String,
    /// La fenêtre (boîte) où il est.
    pub fenetre: Option<Rect>,
    /// Bouton tenu ou souris déplacée avant de lâcher : on tourne la vue, on ne vise rien.
    pub glisse: bool,
}

/// Ce que le journal dit, ramené au temps et aux fractions de la vidéo.
#[derive(Debug, Clone, Default)]
pub struct Gestes {
    /// Clics et saisies.
    pub gestes: Vec<Geste>,
    /// Indice dans `gestes`, bouton.
    pub clics: Vec<(usize, String)>,
    pub touches: Vec<(f64, String)>,
    pub souris: Vec<(f64, f64, f64)>,
    pub ateliers: Vec<(f64, String)>,
    /// Une boîte s'ouvre : instant, titre, rectangle.
    pub boites: Vec<(f64, String, Rect)>,
    /// Chaque fois que la main passe à autre chose : instant, titre, `boite` du journal
    /// (`Some(true)` seulement quand VerdierCAM dit que c'est une boîte).
    pub plans: Vec<(f64, String, Option<bool>)>,
}

fn nombre(e: &Map<String, Value>, nom: &str) -> Option<f64> {
    e.get(nom).filter(|v| v.is_number()).and_then(Value::as_f64)
}

fn texte(e: &Map<String, Value>, nom: &str) -> String {
    match e.get(nom) {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) | Some(Value::Bool(false)) => String::new(),
        Some(v) => v.to_string(),
    }
}

/// `e.get(nom) or ""` : une valeur fausse en Python donne la chaîne vide.
fn texte_ou_vide(e: &Map<String, Value>, nom: &str) -> String {
    match e.get(nom) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn rect_brut(e: &Map<String, Value>) -> Option<[f64; 4]> {
    let r = e.get("rect")?.as_array()?;
    if r.len() != 4 {
        return None;
    }
    let v: Vec<f64> = r.iter().map(|x| x.as_f64()).collect::<Option<_>>()?;
    Some([v[0], v[1], v[2], v[3]])
}

fn mots(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// La taille dans laquelle sont exprimées les coordonnées du journal. VerdierCAM 0.99.446 et
/// 0.99.447 annonçaient la taille demandée au lancement, pas celle de la fenêtre agrandie : leurs
/// coordonnées dépassent la taille annoncée. On prend alors celle de la vidéo.
fn taille_reelle(journal: &Journal, video_l: f64, video_h: f64) -> (f64, f64) {
    let (l, h) = journal.taille_fenetre();
    let (l, h) = (l as f64, h as f64);
    if l == 0.0 || h == 0.0 {
        return (video_l, video_h);
    }
    if journal.evenements.iter().any(|e| e.get("type").and_then(Value::as_str) == Some("taille")) {
        return (l, h);
    }
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for e in &journal.evenements {
        if let Some(x) = nombre(e, "x") {
            max_x = max_x.max(x);
            max_y = max_y.max(nombre(e, "y").unwrap_or(0.0));
        }
        if let Some(r) = rect_brut(e) {
            max_x = max_x.max(r[0] + r[2]);
            max_y = max_y.max(r[1] + r[3]);
        }
    }
    if (max_x > l * 1.02 || max_y > h * 1.02) && video_l != 0.0 && video_h != 0.0 {
        return (video_l, video_h);
    }
    (l, h)
}

/// La taille de l'image filmée, en pixels de la fenêtre : une fenêtre agrandie de KDE ne couvre
/// pas l'écran (la barre des tâches en prend le bas) ; on la pose en haut à gauche, à sa taille.
fn cadre(l: f64, h: f64, video_l: f64, video_h: f64) -> (f64, f64) {
    if l == 0.0 || h == 0.0 || video_l == 0.0 || video_h == 0.0 {
        return (l, h);
    }
    let k = (video_l / l).min(video_h / h);
    (video_l / k, video_h / k)
}

/// Les gestes du journal, au temps et en fractions de la vidéo.
pub fn lire_gestes(journal: &Journal, decalage: f64, video_l: f64, video_h: f64) -> Gestes {
    let (tl, th) = taille_reelle(journal, video_l, video_h);
    let (mut l, mut h) = cadre(tl, th, video_l, video_h);
    let mut g = Gestes::default();
    let mut fenetre: Option<Rect> = None;
    for e in &journal.evenements {
        let t = nombre(e, "t").unwrap_or(0.0) / 1000.0 + decalage;
        let typ = e.get("type").and_then(Value::as_str).unwrap_or("");
        if typ == "taille" {
            let (lp, hp) = (nombre(e, "l_px").unwrap_or(0.0), nombre(e, "h_px").unwrap_or(0.0));
            if lp != 0.0 && hp != 0.0 {
                (l, h) = cadre(lp.trunc(), hp.trunc(), video_l, video_h);
                continue;
            }
        }
        let rect = rect_brut(e).filter(|_| l != 0.0 && h != 0.0).map(|r| [r[0] / l, r[1] / h, r[2] / l, r[3] / h]);
        match typ {
            "fenetre" => fenetre = rect,
            "premier_plan" => {
                // VerdierCAM dit si c'est une boîte ; avant qu'il le dise, la taille en décide. Un
                // rectangle minuscule est un menu à sa première image.
                let titre = mots(&texte(e, "titre"));
                let boite = match e.get("boite") {
                    None | Some(Value::Null) => None,
                    Some(Value::Bool(true)) => Some(true),
                    Some(_) => Some(false),
                };
                g.plans.push((t, titre.clone(), boite));
                if let Some(r) = rect
                    && !titre.is_empty()
                    && r[2] > 0.06
                    && r[3] > 0.06
                    && (boite == Some(true) || (boite.is_none() && est_boite(Some(&r))))
                {
                    g.boites.push((t, titre, r));
                }
            }
            "clic" | "saisie" if e.get("x").is_none_or(Value::is_number) => {
                let (mut x, mut y) = (nombre(e, "x"), nombre(e, "y"));
                if x.is_none()
                    && let Some(r) = rect
                {
                    x = Some((r[0] + r[2] / 2.0) * l);
                    y = Some((r[1] + r[3] / 2.0) * h);
                }
                let Some(x) = x else { continue };
                let geste = Geste {
                    t,
                    x: x / l,
                    y: y.unwrap_or(0.0) / h,
                    rect,
                    libelle: texte_ou_vide(e, "libelle"),
                    fenetre,
                    glisse: false,
                };
                g.gestes.push(geste);
                if typ == "clic" {
                    let bouton = texte_ou_vide(e, "bouton");
                    g.clics.push((g.gestes.len() - 1, if bouton.is_empty() { "gauche".into() } else { bouton }));
                }
            }
            "touche" => {
                let touche = texte(e, "touche");
                if !touche.is_empty() {
                    g.touches.push((t, touche));
                }
            }
            "relache" => {
                if let (Some(&(i, ref bouton)), Some(ex)) = (g.clics.last(), nombre(e, "x")) {
                    let lache = texte_ou_vide(e, "bouton");
                    if *bouton == if lache.is_empty() { "gauche".to_string() } else { lache } {
                        let dernier = &g.gestes[i];
                        let ey = nombre(e, "y").unwrap_or(0.0);
                        let distance = ((ex / l - dernier.x) * l).powi(2) + ((ey / h - dernier.y) * h).powi(2);
                        if t - dernier.t > GLISSER_S || distance > GLISSER_PX * GLISSER_PX {
                            g.gestes[i].glisse = true;
                        }
                    }
                }
            }
            "souris" => {
                if let Some(x) = nombre(e, "x") {
                    g.souris.push((t, x / l, nombre(e, "y").unwrap_or(0.0) / h));
                }
            }
            "atelier" => {
                let nom = texte(e, "nom");
                if !nom.is_empty() {
                    g.ateliers.push((t, nom));
                }
            }
            _ => {}
        }
    }
    g
}

pub fn est_boite(rect: Option<&Rect>) -> bool {
    rect.is_some_and(|r| r[2] < BOITE_MAX && r[3] < BOITE_MAX)
}

/// La part d'écran à montrer pour un geste : le réglage et son voisinage, ou la boîte de
/// dialogue entière quand elle est assez petite pour gagner à être agrandie.
fn region(g: &Geste) -> (f64, f64, f64) {
    if let Some(f) = g.fenetre.filter(|f| est_boite(Some(f))) {
        let cote = f[2].max(f[3]).max(VOISINAGE);
        return (f[0] + f[2] / 2.0, f[1] + f[3] / 2.0, cote * 1.08);
    }
    match g.rect {
        Some(r) => (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, VOISINAGE.max(r[2] * 1.6).max(r[3] * 1.6)),
        None => (g.x, g.y, VOISINAGE),
    }
}

/// La part de l'écran (x, y, l, h en fractions) qu'un zoom montre.
pub fn cadre_zoom(facteur: f64, cx: f64, cy: f64) -> Rect {
    let (x, y, l, h) = rect_source(facteur, cx, cy, 1.0, 1.0);
    [x, y, l, h]
}

fn dans(x: f64, y: f64, c: &Rect, marge: f64) -> bool {
    let (mx, my) = (c[2] * marge, c[3] * marge);
    c[0] + mx <= x && x <= c[0] + c[2] - mx && c[1] + my <= y && y <= c[1] + c[3] - my
}

/// Quand la souris est entrée dans le cadre pour la dernière fois avant le geste.
fn arrivee(souris: &[(f64, f64, f64)], c: &Rect, t_geste: f64) -> Option<f64> {
    let mut entree = None;
    for &(t, x, y) in souris {
        if t < t_geste - AVANT_MAX - 1.0 {
            continue;
        }
        if t > t_geste {
            break;
        }
        if !dans(x, y, c, 0.0) {
            entree = None;
        } else if entree.is_none() {
            entree = Some(t);
        }
    }
    entree
}

/// Quand la souris quitte le cadre après le dernier geste (`None` : elle reste).
fn depart(souris: &[(f64, f64, f64)], c: &Rect, t_dernier: f64) -> Option<f64> {
    for &(t, x, y) in souris {
        if t <= t_dernier + 0.1 {
            continue;
        }
        if t > t_dernier + APRES_MAX + 0.5 {
            break;
        }
        if !dans(x, y, c, MARGE_SORTIE) {
            return Some(t);
        }
    }
    None
}

fn fermer(groupe: &mut Vec<Geste>, souris: &[(f64, f64, f64)], zooms: &mut Vec<Zoom>) {
    if groupe.is_empty() {
        return;
    }
    let regions: Vec<_> = groupe.iter().map(region).collect();
    let x0 = regions.iter().map(|r| r.0 - r.2 / 2.0).fold(f64::INFINITY, f64::min);
    let x1 = regions.iter().map(|r| r.0 + r.2 / 2.0).fold(f64::NEG_INFINITY, f64::max);
    let y0 = regions.iter().map(|r| r.1 - r.2 / 2.0).fold(f64::INFINITY, f64::min);
    let y1 = regions.iter().map(|r| r.1 + r.2 / 2.0).fold(f64::NEG_INFINITY, f64::max);
    let cote = (x1 - x0).max(y1 - y0);
    let facteur = if cote > 0.0 { FACTEUR_PROPOSE_MAX.min(1.0 / cote) } else { FACTEUR_PROPOSE_MAX };
    let groupe_courant = std::mem::take(groupe);
    if facteur < 1.3 {
        return;
    }
    // Un clic sans nom, seul, ne vaut pas un zoom ; plusieurs au même endroit, si.
    if groupe_courant.len() < 2 && !groupe_courant.iter().any(|g| !g.libelle.is_empty() || est_boite(g.fenetre.as_ref())) {
        return;
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let c = cadre_zoom(facteur, cx, cy);
    let (premier, dernier) = (groupe_courant[0].t, groupe_courant[groupe_courant.len() - 1].t);
    let mut debut = premier - AVANT_MAX;
    if !souris.is_empty()
        && let Some(entree) = arrivee(souris, &c, premier)
    {
        debut = entree - 0.1;
    }
    debut = debut.max(premier - AVANT_MAX).min(premier - AVANT_MIN);
    let mut fin = dernier + APRES_MAX;
    if !souris.is_empty() {
        // Le zoom se défait en finissant juste après la sortie : la souris reste en vue.
        fin = depart(souris, &c, dernier).map_or(dernier + APRES_MAX, |s| s + 0.12);
    }
    fin = fin.max(dernier + APRES_MIN).min(dernier + APRES_MAX);
    let mut libelles: Vec<&str> = Vec::new();
    for g in &groupe_courant {
        if !g.libelle.is_empty() && !libelles.contains(&g.libelle.as_str()) {
            libelles.push(&g.libelle);
        }
    }
    let mut z = Zoom::nouveau(arrondi(debut.max(0.0), 3), arrondi(fin, 3));
    z.facteur = arrondi(facteur, 2);
    z.cx = arrondi(cx, 4);
    z.cy = arrondi(cy, 4);
    z.origine = "journal".into();
    z.libelle = libelles.join(" · ").chars().take(80).collect();
    zooms.push(z);
}

/// Regroupe les gestes proches en zooms : un zoom par passage sur une même partie de l'écran.
/// Avec la trajectoire de la souris, le zoom arrive quand elle est arrivée, et repart dès
/// qu'elle s'en va.
pub fn zooms_depuis_gestes(gestes: &[Geste], souris: &[(f64, f64, f64)]) -> Vec<Zoom> {
    let mut zooms = Vec::new();
    let mut groupe: Vec<Geste> = Vec::new();
    for g in gestes {
        if let (Some(premier), Some(dernier)) = (groupe.first(), groupe.last()) {
            let (cx0, cy0, _) = region(premier);
            let (cx1, cy1, _) = region(g);
            let loin = (cx1 - cx0).abs().max((cy1 - cy0).abs()) > REGROUPER_ECART;
            if g.t - dernier.t > REGROUPER_S || loin {
                fermer(&mut groupe, souris, &mut zooms);
            }
        }
        groupe.push(g.clone());
    }
    fermer(&mut groupe, souris, &mut zooms);
    // Deux zooms qui se chevauchent : le premier s'arrête où commence le second.
    for i in 1..zooms.len() {
        if zooms[i - 1].fin > zooms[i].debut {
            zooms[i - 1].fin = zooms[i].debut;
        }
    }
    zooms.retain(|z| z.fin - z.debut >= 1.0);
    zooms
}

/// Les instants (s) où tout l'écran change d'un coup, au début de chaque saut du mouvement.
pub fn changements_d_ecran(mouvement: &[f64]) -> Vec<f64> {
    (0..mouvement.len())
        .filter(|&i| mouvement[i] > CHANGEMENT_ECRAN && (i == 0 || mouvement[i - 1] <= CHANGEMENT_ECRAN))
        .map(|i| (i as f64 + 0.5) / IMAGES_PAR_S)
        .collect()
}

/// Ce qui montre un endroit de l'écran s'arrête quand l'écran change d'un coup : l'étiquette et
/// la mise en lumière d'un réglage cliqué, celles d'une boîte qui se ferme, et un zoom quand une
/// autre fenêtre passe devant VerdierCAM ou que la boîte zoomée se ferme.
pub fn couper_aux_changements(montage: &mut Montage, g: &Gestes, mouvement: &[f64]) {
    let sauts = changements_d_ecran(mouvement);
    if sauts.is_empty() {
        return;
    }
    let mut internes: Vec<f64> = g.plans.iter().map(|p| p.0).chain(g.ateliers.iter().map(|a| a.0)).collect();
    internes.sort_by(f64::total_cmp);
    let explique = |s: f64| internes.iter().any(|&u| (s - u).abs() <= EXPLIQUE_S);
    // La main quitte une boîte (un menu, une fenêtre de dialogue) autour de s.
    let fermeture = |s: f64| {
        g.plans
            .windows(2)
            .any(|p| p[0].2 == Some(true) && p[1].2 != Some(true) && (p[1].0 - s).abs() <= EXPLIQUE_S)
    };
    for e in &mut montage.etiquettes {
        if e.origine != "journal" || (e.genre != "reglage" && e.genre != "fenetre") {
            continue;
        }
        // Une boîte change l'écran en s'ouvrant : on cherche après.
        let fenetre = e.genre == "fenetre";
        let depuis = if fenetre { e.debut + 0.5 } else { e.debut - 0.2 };
        let Some(&saut) = sauts.iter().find(|&&s| depuis < s && s < e.fin) else { continue };
        e.fin = arrondi((saut - 0.05).max(if fenetre { e.debut + 0.1 } else { 0.0 }), 3);
        if !fenetre && e.fin - e.debut < ETIQUETTE_LUE {
            e.debut = arrondi((e.fin - ETIQUETTE_LUE).max(0.0), 3);
        }
    }
    for z in &mut montage.zooms {
        if z.origine != "journal" {
            continue;
        }
        if let Some(&saut) = sauts
            .iter()
            .find(|&&s| z.debut + RAMPE < s && s < z.fin && (!explique(s) || fermeture(s)))
        {
            z.fin = arrondi(saut - 0.05, 3);
        }
    }
    montage.zooms.retain(|z| z.origine != "journal" || z.fin - z.debut >= 0.8);
}

/// Remplit zooms, étiquettes et clics d'après le journal. Ce qu'une analyse précédente avait
/// proposé est remplacé ; ce qu'on a posé à la main reste. `mouvement` (celui de l'image) arrête
/// ce qui montre un endroit de l'écran quand l'écran change d'un coup.
pub fn analyser_journal(journal: &Journal, montage: &mut Montage, video: &Path, mouvement: Option<&[f64]>) {
    let decalage = journal.decalage(video, montage.duree) + montage.reglages.decalage_journal;
    let g = lire_gestes(journal, decalage, montage.largeur as f64, montage.hauteur as f64);
    let duree = montage.duree;
    let dedans = |t: f64| 0.0 <= t && t <= duree;

    // Un zoom suit ce qui a un nom, ce qui se fait dans une boîte de dialogue, et plusieurs clics
    // sans nom au même endroit ; un clic seul sur le dessin, un titre de menu (le haut de
    // l'écran) ne valent pas qu'on zoome. Une boîte qui s'ouvre compte comme un geste en son
    // milieu.
    let mut pour_zooms: Vec<Geste> = g
        .gestes
        .iter()
        .filter(|x| dedans(x.t) && (!x.libelle.is_empty() || est_boite(x.fenetre.as_ref()) || (x.y > 0.06 && !x.glisse)))
        .cloned()
        .collect();
    for (t, titre, r) in &g.boites {
        if dedans(*t) && est_boite(Some(r)) {
            pour_zooms.push(Geste {
                t: *t,
                x: r[0] + r[2] / 2.0,
                y: r[1] + r[3] / 2.0,
                rect: None,
                libelle: titre.clone(),
                fenetre: Some(*r),
                glisse: false,
            });
        }
    }
    pour_zooms.sort_by(|a, b| a.t.total_cmp(&b.t));
    montage.zooms.retain(|z| z.origine != "journal");
    montage.zooms.extend(zooms_depuis_gestes(&pour_zooms, &g.souris));

    let journal_etiquette = |debut: f64, fin: f64, texte: String, genre: &str| Etiquette {
        debut: arrondi(debut, 3),
        fin: arrondi(fin, 3),
        texte,
        x: 0.5,
        y: 0.5,
        genre: genre.into(),
        origine: "journal".into(),
        rect: None,
        autres: Default::default(),
    };
    let arrondir_rect = |r: &Rect| r.iter().map(|&v| arrondi(v, 4)).collect::<Vec<_>>();
    let mut etiquettes: Vec<Etiquette> = Vec::new();
    for (i, _bouton) in &g.clics {
        let x = &g.gestes[*i];
        if x.libelle.is_empty() || !dedans(x.t) {
            continue;
        }
        // Le titre d'un menu qu'on ouvre : l'étiquette de la boîte le dit déjà.
        if g.boites.iter().any(|(tb, titre, _)| *titre == x.libelle && -4.0 <= tb - x.t && tb - x.t <= 0.6) {
            continue;
        }
        if let Some(derniere) = etiquettes.last_mut()
            && derniere.texte == x.libelle
            && x.t - derniere.debut < 4.0
        {
            derniere.fin = arrondi(x.t + DUREE_ETIQUETTE, 3);
            continue;
        }
        let ancre = x.rect.map_or((x.x, x.y), |r| (r[0] + r[2] / 2.0, r[1] + r[3]));
        let mut e = journal_etiquette(x.t, x.t + DUREE_ETIQUETTE, x.libelle.clone(), "reglage");
        e.x = arrondi(ancre.0, 4);
        e.y = arrondi(ancre.1, 4);
        e.rect = x.rect.as_ref().map(arrondir_rect);
        etiquettes.push(e);
    }
    for (t, touche) in &g.touches {
        if dedans(*t) {
            etiquettes.push(journal_etiquette(*t, t + DUREE_TOUCHE, touche.clone(), "touche"));
        }
    }
    // On change d'atelier : tout l'écran change d'un coup, son nom le dit.
    for (t, nom) in &g.ateliers {
        if 0.3 < *t && *t <= duree {
            etiquettes.push(journal_etiquette(*t, t + DUREE_ATELIER, nom.clone(), "atelier"));
        }
    }
    // Une boîte s'ouvre : son titre, sur elle, tant qu'elle a la main.
    for (t, titre, r) in &g.boites {
        if !dedans(*t) {
            continue;
        }
        let mut fin = t + DUREE_ETIQUETTE;
        if let Some(&(ensuite, _, _)) = g
            .plans
            .iter()
            .find(|(u, ti, b)| *u > t + 0.05 && (ti != titre || *b != Some(true)))
        {
            fin = (t + 0.3).max(fin.min(ensuite));
        }
        let mut e = journal_etiquette(*t, fin, titre.clone(), "fenetre");
        e.x = arrondi(r[0] + r[2] / 2.0, 4);
        e.y = arrondi(r[1], 4);
        e.rect = Some(arrondir_rect(r));
        etiquettes.push(e);
    }
    montage.etiquettes.retain(|e| e.origine != "journal");
    montage.etiquettes.extend(etiquettes);
    if let Some(m) = mouvement.filter(|m| !m.is_empty()) {
        couper_aux_changements(montage, &g, m);
    }
    montage.clics = g
        .clics
        .iter()
        .filter(|(i, _)| dedans(g.gestes[*i].t))
        .map(|(i, b)| {
            let x = &g.gestes[*i];
            Clic { t: arrondi(x.t, 3), x: arrondi(x.x, 4), y: arrondi(x.y, 4), bouton: b.clone(), autres: Default::default() }
        })
        .collect();
    montage.journal = journal.chemin.to_string_lossy().into_owned();
    montage.trier();
}

/// Une étiquette de réglage s'arrête 0,2 s avant la phrase suivante (jamais avant 1,5 s, et sans
/// s'allonger : l'écran qui change a pu l'arrêter plus tôt).
pub fn caler_etiquettes_sur_la_voix(montage: &mut Montage) {
    let mut debuts: Vec<f64> = montage.sous_titres.iter().map(|s| s.debut).collect();
    debuts.sort_by(f64::total_cmp);
    for e in &mut montage.etiquettes {
        if e.genre != "reglage" || e.origine != "journal" {
            continue;
        }
        if let Some(&suivante) = debuts.iter().find(|&&d| d > e.debut + 0.3) {
            e.fin = arrondi(e.fin.min((e.debut + 1.5).max(suivante - 0.2)), 3);
        }
    }
}

/// Les chapitres qu'on peut proposer : un par atelier visité, dans l'ordre (le même atelier revenu
/// aussitôt n'en fait pas deux), le premier au début de la prise.
pub fn chapitres_des_ateliers(journal: &Journal, montage: &Montage, video: &Path) -> Vec<crate::montage::Chapitre> {
    let decalage = journal.decalage(video, montage.duree) + montage.reglages.decalage_journal;
    let g = lire_gestes(journal, decalage, montage.largeur as f64, montage.hauteur as f64);
    let mut res: Vec<crate::montage::Chapitre> = Vec::new();
    for (t, nom) in g.ateliers.iter().filter(|(t, _)| *t <= montage.duree) {
        if res.last().is_some_and(|c| c.titre == *nom) {
            continue;
        }
        let t = if res.is_empty() { 0.0 } else { arrondi(*t, 3) };
        res.push(crate::montage::Chapitre { t, titre: nom.clone(), origine: "journal".into(), autres: Default::default() });
    }
    res
}

/// Les instants (s de la vidéo) où l'on fait quelque chose à l'écran ; `gestes_seuls` : les
/// clics, touches et saisies, sans les mouvements de souris.
pub fn instants_actifs(journal: &Journal, decalage: f64, gestes_seuls: bool) -> Vec<f64> {
    let types: &[&str] =
        if gestes_seuls { &["clic", "relache", "touche", "saisie"] } else { &["souris", "clic", "relache", "touche", "saisie"] };
    journal
        .evenements
        .iter()
        .filter(|e| e.get("type").and_then(Value::as_str).is_some_and(|t| types.contains(&t)))
        .map(|e| nombre(e, "t").unwrap_or(0.0) / 1000.0 + decalage)
        .collect()
}

// ---------------------------------------------------------------------------
//  L'image qui bouge : calage du journal
// ---------------------------------------------------------------------------

/// La commande qui sort l'image grise de la vidéo, 10 fois par seconde, en 96 × 60.
pub fn commande_image_grise(video: &Path) -> Vec<String> {
    vec![
        "ffmpeg".into(),
        "-v".into(),
        "error".into(),
        "-i".into(),
        video.to_string_lossy().into_owned(),
        "-an".into(),
        "-vf".into(),
        format!("fps={IMAGES_PAR_S},scale={GRIS_L}:{GRIS_H},format=gray"),
        "-f".into(),
        "rawvideo".into(),
        "-".into(),
    ]
}

/// Combien l'image change d'un dixième de seconde au suivant (0–255 en moyenne par point) : la
/// valeur i vaut pour l'instant (i + 1) / 10 s.
pub fn mouvement_image(octets: &[u8]) -> Vec<f64> {
    let taille = GRIS_L * GRIS_H;
    let n = octets.len() / taille;
    if n < 2 {
        return Vec::new();
    }
    (0..n - 1)
        .map(|i| {
            let (a, b) = (&octets[i * taille..(i + 1) * taille], &octets[(i + 1) * taille..(i + 2) * taille]);
            let somme: u64 = a.iter().zip(b).map(|(&p, &q)| (p as i16 - q as i16).unsigned_abs() as u64).sum();
            arrondi(somme as f64 / taille as f64, 3)
        })
        .collect()
}

/// Le décalage (s) à ajouter aux instants du journal pour que les clics tombent là où l'écran
/// change. OBS nomme la vidéo à la seconde près : le journal peut tomber jusqu'à une seconde trop
/// tard. `None` si rien ne ressort nettement.
pub fn calage_automatique(mouvement: &[f64], instants: &[f64], cherche: f64, pas: f64) -> Option<f64> {
    let n = mouvement.len() as i64;
    if n < 20 || instants.len() < 3 {
        return None;
    }
    let score = |dec: f64| -> f64 {
        instants
            .iter()
            .filter_map(|&t| {
                let k = entier((t + dec) * IMAGES_PAR_S) - 1;
                (1 <= k && k < n - 1).then(|| {
                    let k = k as usize;
                    mouvement[k - 1].max(mouvement[k]).max(mouvement[k + 1])
                })
            })
            .sum()
    };
    let bout = (cherche / pas) as i64;
    let essais: Vec<(f64, f64)> = (-bout..=bout).map(|i| (score(i as f64 * pas), arrondi(i as f64 * pas, 2))).collect();
    // `max` de Python sur des paires : le score, puis le décalage.
    let &(meilleur, dec) = essais
        .iter()
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .expect("au moins un essai");
    let mut valeurs: Vec<f64> = essais.iter().map(|e| e.0).collect();
    valeurs.sort_by(f64::total_cmp);
    let mediane = valeurs[valeurs.len() / 2];
    if meilleur <= 0.0 || meilleur < 1.15 * mediane {
        return None;
    }
    Some(dec)
}

pub fn mouvement_moyen(mouvement: &[f64], a: f64, b: f64) -> f64 {
    let i0 = ((a * IMAGES_PAR_S) as i64).max(0) as usize;
    let i1 = ((b * IMAGES_PAR_S) as i64).clamp(0, mouvement.len() as i64) as usize;
    if i0 >= i1 {
        return 0.0;
    }
    let morceau = &mouvement[i0..i1];
    morceau.iter().sum::<f64>() / morceau.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal(lignes: &[Value]) -> Journal {
        Journal {
            chemin: "gestes essai.jsonl".into(),
            entete: serde_json::json!({"type": "entete", "debut_ms": 0, "fenetre": {"l_px": 1000, "h_px": 1000}})
                .as_object()
                .unwrap()
                .clone(),
            evenements: lignes.iter().map(|l| l.as_object().unwrap().clone()).collect(),
            debut: 0.0,
        }
    }

    /// Un clic tenu plus de 0,45 s, ou lâché à plus de 25 px, est un glisser : on tourne la vue.
    #[test]
    fn glisser_au_temps_et_a_la_distance() {
        use serde_json::json;
        let cas = [
            (300, 500.0, false), // court et sur place : un clic
            (500, 500.0, true),  // tenu 0,5 s
            (300, 530.0, true),  // lâché 30 px plus loin
            (440, 524.0, false), // juste sous les deux seuils
        ];
        for (duree, x_lache, glisse) in cas {
            let j = journal(&[
                json!({"t": 1000, "type": "clic", "x": 500.0, "y": 500.0, "bouton": "gauche"}),
                json!({"t": 1000 + duree, "type": "relache", "x": x_lache, "y": 500.0, "bouton": "gauche"}),
            ]);
            let g = lire_gestes(&j, 0.0, 1000.0, 1000.0);
            assert_eq!(g.gestes[0].glisse, glisse, "tenu {duree} ms, lâché à {x_lache}");
        }
        // Un autre bouton lâché ne dit rien du clic gauche.
        let j = journal(&[
            json!({"t": 1000, "type": "clic", "x": 500.0, "y": 500.0}),
            json!({"t": 2000, "type": "relache", "x": 900.0, "y": 500.0, "bouton": "droit"}),
        ]);
        assert!(!lire_gestes(&j, 0.0, 1000.0, 1000.0).gestes[0].glisse);
    }
}
