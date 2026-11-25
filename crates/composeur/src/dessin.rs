//! L'image montée d'un instant : le zoom, la mise en lumière du réglage cliqué, les clics, les
//! étiquettes, le passage accéléré, les sous-titres. Repris trait pour trait de `compositeur.py`
//! du Monteur 1 (QPainter), mêmes tailles, mêmes durées, mêmes couleurs.
//!
//! Le dessin se fait en deux couches : l'image de la vidéo (zoomée) puis le calque de ce qui se
//! pose dessus. L'aperçu et le rendu passent tous deux par ici : ce qu'on voit est ce qui sort.

use noyau::montage::{self, Montage};
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapMut, PixmapPaint,
    PixmapRef, Rect, Stroke, Transform,
};

use crate::police::{Polices, taille};

/// Le cercle d'un clic s'élargit et s'efface en tant de secondes.
pub const DUREE_CLIC: f64 = 0.6;
/// Apparition et disparition d'une étiquette.
pub const FONDU: f64 = 0.2;
/// L'étiquette arrive en grossissant un peu plus que sa taille.
pub const BOND: f64 = 0.22;
/// Le reste de l'image, assombri d'autant autour d'un réglage cliqué.
pub const OMBRE_LUMIERE: f64 = 0.5;

const ORANGE: (u8, u8, u8) = (0xFF, 0x6D, 0x00);
const FOND: (u8, u8, u8) = (0x1E, 0x1E, 0x1E);
const BLANC: (u8, u8, u8) = (255, 255, 255);
const NOIR: (u8, u8, u8) = (0, 0, 0);

fn couleur((r, g, b): (u8, u8, u8), alpha: f64) -> Color {
    Color::from_rgba8(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn peinture(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

/// Le crayon par défaut de Qt : bouts carrés, angles biseautés.
fn crayon(largeur: f64) -> Stroke {
    Stroke { width: largeur as f32, line_cap: LineCap::Square, line_join: LineJoin::Bevel, ..Stroke::default() }
}

/// 1 au milieu, fondu à l'entrée et à la sortie.
fn opacite(t: f64, debut: f64, fin: f64) -> f64 {
    if t < debut || t > fin {
        return 0.0;
    }
    ((t - debut) / FONDU).min((fin - t) / FONDU).clamp(0.0, 1.0)
}

/// L'échelle d'une étiquette qui arrive : 0,8 → 1,06 → 1.
fn bond(t: f64, debut: f64) -> f64 {
    let u = (t - debut) / BOND;
    if u >= 1.0 {
        1.0
    } else if u < 0.6 {
        0.8 + 0.26 * (u / 0.6)
    } else {
        1.06 - 0.06 * ((u - 0.6) / 0.4)
    }
}

/// Un rectangle de flottants, comme `QRectF`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct R {
    pub x: f64,
    pub y: f64,
    pub l: f64,
    pub h: f64,
}

impl R {
    pub fn new(x: f64, y: f64, l: f64, h: f64) -> Self {
        Self { x, y, l, h }
    }
    fn droite(&self) -> f64 {
        self.x + self.l
    }
    fn bas(&self) -> f64 {
        self.y + self.h
    }
    fn centre(&self) -> (f64, f64) {
        (self.x + self.l / 2.0, self.y + self.h / 2.0)
    }
    fn agrandi(&self, m: f64) -> Self {
        Self::new(self.x - m, self.y - m, self.l + 2.0 * m, self.h + 2.0 * m)
    }
    fn skia(&self) -> Option<Rect> {
        Rect::from_xywh(self.x as f32, self.y as f32, self.l as f32, self.h as f32)
    }
}

/// `QPainterPath.addRoundedRect` : quatre quarts de cercle en courbes de Bézier.
fn rect_arrondi(pb: &mut PathBuilder, r: R, rayon: f64) {
    let rayon = rayon.min(r.l / 2.0).min(r.h / 2.0).max(0.0) as f32;
    let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, r.droite() as f32, r.bas() as f32);
    let k = rayon * (1.0 - 0.552_284_8);
    pb.move_to(x1 - rayon, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + rayon);
    pb.line_to(x1, y1 - rayon);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - rayon, y1);
    pb.line_to(x0 + rayon, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - rayon);
    pb.line_to(x0, y0 + rayon);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + rayon, y0);
    pb.close();
}

fn chemin_arrondi(r: R, rayon: f64) -> Option<Path> {
    let mut pb = PathBuilder::new();
    rect_arrondi(&mut pb, r, rayon);
    pb.finish()
}

fn cercle(cx: f64, cy: f64, rayon: f64) -> Option<Path> {
    PathBuilder::from_circle(cx as f32, cy as f32, rayon as f32)
}

pub struct Compositeur<'a> {
    pub montage: &'a Montage,
    pub polices: &'a Polices,
    cles: Vec<montage::Cle>,
}

/// Où peindre : la cible dans l'image de sortie, la part de la source montrée, la taille de la
/// source, et l'échelle (tout se mesure à la hauteur de la cible : le même montage rend pareil en
/// petit dans l'aperçu et en grand dans la vidéo).
struct Cadre {
    cible: R,
    source: (f64, f64, f64, f64),
    largeur: f64,
    hauteur: f64,
    e: f64,
}

impl Cadre {
    /// Un point de la vidéo (fractions) dans la cible, à travers le zoom.
    fn vers_cible(&self, fx: f64, fy: f64) -> (f64, f64) {
        let (x, y, l, h) = self.source;
        (
            self.cible.x + (fx * self.largeur - x) / l * self.cible.l,
            self.cible.y + (fy * self.hauteur - y) / h * self.cible.h,
        )
    }

    fn rect_cible(&self, r: &[f64]) -> R {
        let (ax, ay) = self.vers_cible(r[0], r[1]);
        let (bx, by) = self.vers_cible(r[0] + r[2], r[1] + r[3]);
        R::new(ax, ay, bx - ax, by - ay)
    }
}

impl<'a> Compositeur<'a> {
    pub fn new(montage: &'a Montage, polices: &'a Polices) -> Self {
        // Zooms décochés : aucune clé, l'image reste entière dans l'aperçu comme au rendu.
        let cles = if montage.reglages.montrer_zooms { montage::cles_zoom(&montage.zooms) } else { Vec::new() };
        Self { montage, polices, cles }
    }

    /// La part de l'image source montrée à l'instant t (x, y, l, h en pixels).
    pub fn vue(&self, t: f64, largeur: f64, hauteur: f64) -> (f64, f64, f64, f64) {
        let (f, cx, cy) = montage::etat_zoom(&self.cles, t);
        montage::rect_source(f, cx, cy, largeur, hauteur)
    }

    /// Toute l'image d'un instant : `image` (celle de la vidéo à t) posée dans `cible`, puis le
    /// calque.
    pub fn peindre(&self, sortie: &mut PixmapMut, image: Option<PixmapRef>, t: f64, cible: R, sans_zoom: bool) {
        self.peindre_avec_webcam(sortie, image, None, t, cible, sans_zoom);
    }

    /// Comme `peindre`, avec l'image de la webcam à t : elle se pose sur l'écran et ce qui l'accompagne (zoom,
    /// clics, étiquettes), sous les sous-titres, qui continuent quand elle passe en grand.
    pub fn peindre_avec_webcam(&self, sortie: &mut PixmapMut, image: Option<PixmapRef>, webcam: Option<PixmapRef>, t: f64, cible: R, sans_zoom: bool) {
        let m = self.montage;
        let largeur = image.map_or(0, |i| i.width()) as f64;
        let hauteur = image.map_or(0, |i| i.height()) as f64;
        let largeur = if largeur > 0.0 { largeur } else if m.largeur > 0 { m.largeur as f64 } else { 1.0 };
        let hauteur = if hauteur > 0.0 { hauteur } else if m.hauteur > 0 { m.hauteur as f64 } else { 1.0 };
        let source = if sans_zoom { (0.0, 0.0, largeur, hauteur) } else { self.vue(t, largeur, hauteur) };
        if let Some(image) = image {
            peindre_image(sortie, image, cible, source);
        }
        let Some((w, camera)) = m.webcam.as_ref().filter(|w| w.montrer).zip(webcam) else {
            self.peindre_calque(sortie, t, cible, source, largeur, hauteur);
            return;
        };
        let c = Cadre { cible, source, largeur, hauteur, e: cible.h / 1080.0 };
        let masque = masque_cible(sortie.width(), sortie.height(), cible);
        let mut p = Peintre { sortie, masque: masque.as_ref() };
        self.calque_ecran(&mut p, t, &c);
        peindre_webcam(p.sortie, w, camera, t, cible);
        if m.sous_titres_incrustes() {
            self.sous_titres(&mut p, t, &c);
        }
    }

    /// Ce qui se pose sur l'image : à peindre par-dessus l'image zoomée, ou sur un calque
    /// transparent que la carte graphique pose ensuite sur elle.
    pub fn peindre_calque(
        &self,
        sortie: &mut PixmapMut,
        t: f64,
        cible: R,
        source: (f64, f64, f64, f64),
        largeur: f64,
        hauteur: f64,
    ) {
        let c = Cadre { cible, source, largeur, hauteur, e: cible.h / 1080.0 };
        let masque = masque_cible(sortie.width(), sortie.height(), cible);
        let mut p = Peintre { sortie, masque: masque.as_ref() };
        self.calque_ecran(&mut p, t, &c);
        // Mis à part dans un .srt, ils ne se peignent ni dans l'aperçu ni dans le rendu.
        if self.montage.sous_titres_incrustes() {
            self.sous_titres(&mut p, t, &c);
        }
    }

    /// Ce qui parle de l'écran filmé : la mise en lumière, les clics, les étiquettes, le passage accéléré.
    fn calque_ecran(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        let r = &self.montage.reglages;
        if r.montrer_etiquettes && r.mettre_en_lumiere {
            self.lumiere(p, t, c);
        }
        if r.montrer_clics {
            self.clics(p, t, c);
        }
        if r.montrer_etiquettes {
            self.etiquettes(p, t, c);
        }
        self.vitesse(p, t, c);
    }

    /// Le réglage cliqué (ou la boîte qui s'ouvre) ressort : le reste s'assombrit, un cadre orange
    /// l'entoure.
    fn lumiere(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        for e in &self.montage.etiquettes {
            let Some(rect) = e.rect.as_ref().filter(|r| r.len() == 4) else { continue };
            if e.genre != "reglage" && e.genre != "fenetre" {
                continue;
            }
            let a = opacite(t, e.debut, e.fin);
            if a <= 0.0 {
                continue;
            }
            let marge = if e.genre == "reglage" { 8.0 } else { 4.0 } * c.e;
            let trou = c.rect_cible(rect).agrandi(marge);
            let mut pb = PathBuilder::new();
            if let Some(tout) = c.cible.skia() {
                pb.push_rect(tout);
            }
            rect_arrondi(&mut pb, trou, 8.0 * c.e);
            let voile = OMBRE_LUMIERE * a * if e.genre == "fenetre" { 0.75 } else { 1.0 };
            if let Some(chemin) = pb.finish() {
                p.remplir(&chemin, couleur(NOIR, voile), FillRule::EvenOdd, Transform::identity());
            }
            if let Some(chemin) = chemin_arrondi(trou, 8.0 * c.e) {
                p.tracer(&chemin, couleur(ORANGE, a), (4.0 * c.e).max(1.5), Transform::identity());
            }
        }
    }

    fn clics(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        for clic in &self.montage.clics {
            if !(clic.t <= t && t <= clic.t + DUREE_CLIC) {
                continue;
            }
            let u = (t - clic.t) / DUREE_CLIC;
            let (cx, cy) = c.vers_cible(clic.x, clic.y);
            let rayon = (14.0 + 36.0 * (1.0 - (1.0 - u).powi(2))) * c.e;
            let Some(rond) = cercle(cx, cy, rayon) else { continue };
            // Un liseré blanc sous l'orange : le cercle se voit sur un fond orange aussi.
            p.tracer(&rond, couleur(BLANC, 0.85 * (1.0 - u)), (9.0 * c.e).max(2.0), Transform::identity());
            p.tracer(&rond, couleur(ORANGE, 1.0 - u), (5.5 * c.e).max(1.5), Transform::identity());
            if u < 0.35
                && let Some(plein) = cercle(cx, cy, 9.0 * c.e)
            {
                p.remplir(&plein, couleur(ORANGE, 0.7 * (1.0 - u / 0.35)), FillRule::Winding, Transform::identity());
            }
        }
    }

    /// Une étiquette sombre à filet orange ; rend son rectangle (avant le bond).
    #[allow(clippy::too_many_arguments)]
    fn badge(&self, p: &mut Peintre, texte: &str, (x, y): (f64, f64), c: &Cadre, opac: f64, taille_px: f64, filet: bool, bond: f64) -> R {
        let e = c.e;
        let px = taille(taille_px * e);
        let police = &self.polices.grasse;
        let (marge_x, marge_y) = (20.0 * e, 11.0 * e);
        let filet_l = if filet { 8.0 * e } else { 0.0 };
        let l = police.avance(texte, px) as f64 + 2.0 * marge_x + filet_l;
        let h = police.hauteur(px) as f64 + 2.0 * marge_y;
        let rect = R::new(x, y, l, h);
        let tr = if bond != 1.0 {
            let (cx, cy) = rect.centre();
            Transform::from_translate(cx as f32, cy as f32).pre_scale(bond as f32, bond as f32).pre_translate(-cx as f32, -cy as f32)
        } else {
            Transform::identity()
        };
        // L'ombre portée détache l'étiquette d'une interface sombre.
        if let Some(ombre) = chemin_arrondi(R::new(x, y + 4.0 * e, l, h), 9.0 * e) {
            p.remplir(&ombre, couleur(NOIR, 110.0 / 255.0 * opac), FillRule::Winding, tr);
        }
        let Some(chemin) = chemin_arrondi(rect, 9.0 * e) else { return rect };
        p.remplir(&chemin, couleur(FOND, 0.96 * opac), FillRule::Winding, tr);
        p.tracer(&chemin, couleur(ORANGE, opac), (2.5 * e).max(1.0), tr);
        if filet && let Some(bande) = R::new(x, y, filet_l, h).skia() {
            p.remplir_dans(&PathBuilder::from_rect(bande), &chemin, couleur(ORANGE, opac), tr);
        }
        // Centré en hauteur dans la boîte, aligné à gauche après le filet et la marge.
        let haut_texte = y + (h - police.hauteur(px) as f64) / 2.0;
        let ligne = haut_texte + police.ascent(px) as f64;
        if let Some(lettres) = police.chemin(texte, px, (x + filet_l + marge_x) as f32, ligne as f32) {
            p.remplir(&lettres, couleur(BLANC, opac), FillRule::Winding, tr);
        }
        rect
    }

    fn etiquettes(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        let e = c.e;
        let grasse = &self.polices.grasse;
        for et in &self.montage.etiquettes {
            let a = opacite(t, et.debut, et.fin);
            if a <= 0.0 || et.texte.trim().is_empty() {
                continue;
            }
            let texte = et.texte.split_whitespace().collect::<Vec<_>>().join(" ");
            let b = bond(t, et.debut);
            match et.genre.as_str() {
                "touche" => {
                    // Un raccourci : en bas à gauche, comme une touche du clavier.
                    let h = grasse.hauteur(taille(42.0 * e)) as f64 + 22.0 * e;
                    let coin = (c.cible.x + 0.035 * c.cible.l, c.cible.y + 0.80 * c.cible.h - h);
                    self.badge(p, &texte, coin, c, a, 42.0, false, b);
                }
                "atelier" => {
                    // On change d'atelier : son nom en haut, au milieu, en grand.
                    let l = grasse.avance(&texte, taille(52.0 * e)) as f64 + 48.0 * e;
                    let coin = (c.cible.centre().0 - l / 2.0, c.cible.y + 0.11 * c.cible.h);
                    self.badge(p, &texte, coin, c, a, 52.0, true, b);
                }
                genre => {
                    let point = c.vers_cible(et.x, et.y);
                    let px = taille(38.0 * e);
                    let l = grasse.avance(&texte, px) as f64 + 48.0 * e;
                    let h = grasse.hauteur(px) as f64 + 22.0 * e;
                    if genre == "fenetre" {
                        // Le titre d'une boîte qui s'ouvre : posé sur son bord haut.
                        let x = (c.cible.x + 8.0 * e).max(point.0 - l / 2.0).min(c.cible.droite() - l - 8.0 * e);
                        let y = (c.cible.y + 8.0 * e).max(point.1 - h / 2.0);
                        self.badge(p, &texte, (x, y), c, a, 38.0, true, b);
                        continue;
                    }
                    // Sous le réglage, un peu décalée ; au-dessus s'il n'y a pas la place.
                    let x = (c.cible.x + 8.0 * e).max(point.0 - 28.0 * e).min(c.cible.droite() - l - 8.0 * e);
                    let mut y = point.1 + 18.0 * e;
                    if y + h > c.cible.bas() - 0.16 * c.cible.h {
                        y = point.1 - h - 54.0 * e; // garde la place des sous-titres
                    }
                    let y = (c.cible.y + 8.0 * e).max(y);
                    self.badge(p, &texte, (x, y), c, a, 38.0, true, b);
                    // La pointe vers le réglage.
                    let depart = (point.0.max(x + 12.0 * e).min(x + l - 12.0 * e), if y > point.1 { y } else { y + h });
                    let mut pb = PathBuilder::new();
                    pb.move_to(depart.0 as f32, depart.1 as f32);
                    pb.line_to(point.0 as f32, point.1 as f32);
                    if let Some(trait_) = pb.finish() {
                        p.tracer(&trait_, couleur(ORANGE, a), (3.5 * e).max(1.0), Transform::identity());
                    }
                    if let Some(bout) = cercle(point.0, point.1, 6.0 * e) {
                        p.remplir(&bout, couleur(ORANGE, a), FillRule::Winding, Transform::identity());
                    }
                }
            }
        }
    }

    /// Un passage accéléré le dit, en haut à droite.
    fn vitesse(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        let v = self.montage.vitesse_a(t);
        if v <= 1.0 {
            return;
        }
        let nombre = if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") };
        let texte = format!("▶▶ ×{}", nombre.replace('.', ","));
        let l = self.polices.grasse.avance(&texte, taille(34.0 * c.e)) as f64 + 40.0 * c.e;
        let coin = (c.cible.droite() - l - 0.03 * c.cible.l, c.cible.y + 0.04 * c.cible.h);
        self.badge(p, &texte, coin, c, 0.95, 34.0, false, 1.0);
    }

    /// La boîte du sous-titre de l'instant t dans `cible`, ses lignes, leur hauteur et la taille
    /// du texte ; `None` s'il n'y en a pas.
    fn mise_en_page(&self, t: f64, cible: R) -> Option<(R, Vec<(String, f32)>, f64, f32)> {
        let e = cible.h / 1080.0;
        let r = &self.montage.reglages;
        let s = self.montage.sous_titres.iter().find(|s| s.debut <= t && t <= s.fin && !s.texte.trim().is_empty())?;
        let px = taille(r.taille_sous_titres * 1080.0 * e);
        let police = &self.polices.grasse;
        let lignes = couper_en_lignes(police, s.texte.trim(), px, (cible.l * 0.84) as f32);
        let large = lignes.iter().map(|l| l.1).fold(0.0f32, f32::max) as f64;
        // `QTextLine.height()` : la hauteur de la police arrondie au pixel du dessus (61 pour
        // 60,48 à 50 px), mesuré sur Qt 6 le 04/11/2026.
        let hauteur_ligne = (police.hauteur(px) as f64).ceil();
        let haut = hauteur_ligne * lignes.len() as f64;
        let (marge_x, marge_y) = (22.0 * e, 10.0 * e);
        let rect = R::new(
            cible.centre().0 - large / 2.0 - marge_x,
            cible.bas() - 0.06 * cible.h - haut - 2.0 * marge_y,
            large + 2.0 * marge_x,
            haut + 2.0 * marge_y,
        );
        Some((rect, lignes, hauteur_ligne, px))
    }

    /// La boîte du sous-titre incrusté à l'instant t, s'il y en a un.
    pub fn boite_sous_titre(&self, t: f64, cible: R) -> Option<R> {
        self.montage.sous_titres_incrustes().then(|| self.mise_en_page(t, cible)).flatten().map(|m| m.0)
    }

    fn sous_titres(&self, p: &mut Peintre, t: f64, c: &Cadre) {
        let e = c.e;
        let Some((rect, lignes, hauteur_ligne, px)) = self.mise_en_page(t, c.cible) else { return };
        let police = &self.polices.grasse;
        let large = lignes.iter().map(|l| l.1).fold(0.0f32, f32::max) as f64;
        let (marge_x, marge_y) = (22.0 * e, 10.0 * e);
        if let Some(fond) = chemin_arrondi(rect, 8.0 * e) {
            p.remplir(&fond, couleur(NOIR, 0.72), FillRule::Winding, Transform::identity());
        }
        for (i, (texte, largeur)) in lignes.iter().enumerate() {
            let x = rect.x + marge_x + (large - *largeur as f64) / 2.0;
            let base = rect.y + marge_y + i as f64 * hauteur_ligne + police.ascent(px) as f64;
            if let Some(lettres) = police.chemin(texte, px, x as f32, base as f32) {
                p.remplir(&lettres, couleur(BLANC, 1.0), FillRule::Winding, Transform::identity());
            }
        }
    }
}

/// Où la webcam se pose dans `cible` à l'instant t (pixels de la sortie), pour l'aperçu qui la déplace à la souris.
pub fn rect_webcam(w: &noyau::montage::Webcam, t: f64, cible: R, camera: (f64, f64)) -> R {
    let ((x, y, l, h), _) = w.place(t, cible.l, cible.h, camera.0, camera.1);
    R::new(cible.x + x, cible.y + y, l, h)
}

/// La webcam : une vignette aux coins arrondis, cernée d'un filet clair, qui s'élargit jusqu'à toute l'image en
/// perdant ses coins et son filet.
fn peindre_webcam(sortie: &mut PixmapMut, w: &noyau::montage::Webcam, camera: PixmapRef, t: f64, cible: R) {
    let (cl, ch) = (camera.width() as f64, camera.height() as f64);
    let ((x, y, l, h), source) = w.place(t, cible.l, cible.h, cl, ch);
    let place = R::new(cible.x + x, cible.y + y, l, h);
    let petite = 1.0 - w.grandeur(t);
    let e = cible.h / 1080.0;
    let rayon = 14.0 * e * petite;
    let Some(forme) = chemin_arrondi(place, rayon) else { return };
    // Une ombre douce sous la vignette : elle se détache d'un écran clair comme d'un sombre.
    if petite > 0.0 {
        for (marge, alpha) in [(6.0, 0.10), (3.0, 0.16)] {
            if let Some(ombre) = chemin_arrondi(place.agrandi(marge * e), rayon + marge * e) {
                sortie.fill_path(&ombre, &peinture(couleur(NOIR, alpha * petite)), FillRule::Winding, Transform::identity(), None);
            }
        }
    }
    let Some(mut masque) = Mask::new(sortie.width(), sortie.height()) else { return };
    masque.fill_path(&forme, FillRule::Winding, true, Transform::identity());
    let (sx, sy, sl, sh) = source;
    let tr = Transform::from_translate(place.x as f32, place.y as f32)
        .pre_scale((place.l / sl) as f32, (place.h / sh) as f32)
        .pre_translate(-sx as f32, -sy as f32);
    let peinture_image = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    sortie.draw_pixmap(0, 0, camera, &peinture_image, tr, Some(&masque));
    if petite > 0.0 {
        sortie.stroke_path(&forme, &peinture(couleur(BLANC, 0.85 * petite)), &crayon((2.5 * e).max(1.0)), Transform::identity(), None);
    }
}

/// L'image de la vidéo posée dans `cible`, la part `source` agrandie (lissage bilinéaire, comme
/// `SmoothPixmapTransform` de Qt). L'aperçu le fait faire à la carte graphique (coordonnées de
/// texture), le rendu ici.
pub fn peindre_image(sortie: &mut PixmapMut, image: PixmapRef, cible: R, source: (f64, f64, f64, f64)) {
    let masque = masque_cible(sortie.width(), sortie.height(), cible);
    let (sx, sy, sl, sh) = source;
    let tr = Transform::from_translate(cible.x as f32, cible.y as f32)
        .pre_scale((cible.l / sl) as f32, (cible.h / sh) as f32)
        .pre_translate(-sx as f32, -sy as f32);
    let peinture = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    sortie.draw_pixmap(0, 0, image, &peinture, tr, masque.as_ref());
}

/// Le texte coupé en lignes d'au plus `largeur_max` pixels, entre deux mots (un mot trop long
/// pour une ligne se coupe où il faut), comme `QTextLayout` en `WrapAtWordBoundaryOrAnywhere`.
/// Chaque ligne avec sa largeur, sans les espaces de la fin.
pub fn couper_en_lignes(police: &crate::police::Police, texte: &str, px: f32, largeur_max: f32) -> Vec<(String, f32)> {
    let mut lignes = Vec::new();
    let mut courante = String::new();
    for mot in texte.split(' ') {
        let essai = if courante.is_empty() { mot.to_string() } else { format!("{courante} {mot}") };
        if courante.is_empty() || police.avance(essai.trim_end(), px) <= largeur_max {
            courante = essai;
            continue;
        }
        lignes.push(std::mem::take(&mut courante));
        courante = mot.to_string();
    }
    lignes.push(courante);
    // Un mot seul trop long : coupé à la lettre.
    let mut res = Vec::new();
    for ligne in lignes {
        let mut reste = ligne.trim_end().to_string();
        while police.avance(&reste, px) > largeur_max && reste.chars().count() > 1 {
            let mut n = reste.chars().count() - 1;
            while n > 1 && police.avance(&reste.chars().take(n).collect::<String>(), px) > largeur_max {
                n -= 1;
            }
            let tete: String = reste.chars().take(n).collect();
            res.push(tete.clone());
            reste = reste.chars().skip(n).collect();
        }
        res.push(reste);
    }
    res.into_iter().map(|l| (l.clone(), police.avance(&l, px))).collect()
}

fn masque_cible(l: u32, h: u32, cible: R) -> Option<Mask> {
    if cible.x <= 0.0 && cible.y <= 0.0 && cible.droite() >= l as f64 && cible.bas() >= h as f64 {
        return None;
    }
    let mut m = Mask::new(l, h)?;
    m.fill_path(&PathBuilder::from_rect(cible.skia()?), FillRule::Winding, false, Transform::identity());
    Some(m)
}

struct Peintre<'p, 'q> {
    sortie: &'p mut PixmapMut<'q>,
    masque: Option<&'p Mask>,
}

impl Peintre<'_, '_> {
    fn remplir(&mut self, chemin: &Path, c: Color, regle: FillRule, tr: Transform) {
        self.sortie.fill_path(chemin, &peinture(c), regle, tr, self.masque);
    }

    fn tracer(&mut self, chemin: &Path, c: Color, largeur: f64, tr: Transform) {
        self.sortie.stroke_path(chemin, &peinture(c), &crayon(largeur), tr, self.masque);
    }

    /// Remplit `forme` seulement dans `dedans` (le filet orange pris dans l'arrondi).
    fn remplir_dans(&mut self, forme: &Path, dedans: &Path, c: Color, tr: Transform) {
        let Some(mut m) = Mask::new(self.sortie.width(), self.sortie.height()) else { return };
        m.fill_path(dedans, FillRule::Winding, true, tr);
        if let Some(cible) = self.masque {
            // Les deux à la fois : le plus petit des deux masques, point par point.
            for (a, b) in m.data_mut().iter_mut().zip(cible.data()) {
                *a = (*a).min(*b);
            }
        }
        self.sortie.fill_path(forme, &peinture(c), FillRule::Winding, tr, Some(&m));
    }
}

/// Une image noire de la taille voulue (le Monteur 1 : `image_vide`).
pub fn image_vide(l: u32, h: u32) -> Pixmap {
    let mut p = Pixmap::new(l.max(1), h.max(1)).expect("taille non nulle");
    p.fill(Color::BLACK);
    p
}

#[cfg(test)]
mod tests_zooms {
    use super::*;

    /// « Montrer les zooms » décochée : l'image reste entière au milieu du zoom, qui reste dans le montage.
    #[test]
    fn zooms_decoches_image_entiere() {
        let mut m = Montage { duree: 10.0, largeur: 1920, hauteur: 1080, ..Montage::default() };
        m.zooms.push(noyau::montage::Zoom { debut: 2.0, fin: 8.0, facteur: 2.0, cx: 0.25, cy: 0.25, origine: "main".into(), libelle: String::new(), autres: Default::default() });
        let polices = Polices::charger();
        let (_, _, l, _) = Compositeur::new(&m, &polices).vue(5.0, 1920.0, 1080.0);
        assert!(l < 1000.0, "cochée, le zoom resserre : {l}");
        m.reglages.montrer_zooms = false;
        assert_eq!(Compositeur::new(&m, &polices).vue(5.0, 1920.0, 1080.0), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(m.zooms.len(), 1);
    }
}
