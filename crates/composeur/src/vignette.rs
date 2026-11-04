//! La vignette YouTube, à la charte validée le 23/10/2026 (vignettes FILETAGE et MACHINE) : bandeau
//! #1E1E1E à gauche avec son filet orange, logo et « VerdierCAM », titre blanc en Inter SemiBold
//! 112, sous-titre en Inter Regular 42, trait orange, trois lignes orange en Inter SemiBold 36 ;
//! l'image de la vidéo à droite, sur 646 × 720. Positions, corps et couleurs : ceux de
//! `noyau.STYLES` du Monteur 1, qui la faisait avec ImageMagick (corps en points à 72 ppp = pixels,
//! y = ligne de base).

use noyau::fiches::Fiche;
use noyau::{IMAGE_H, IMAGE_L, VIGNETTE_H, VIGNETTE_L};
use tiny_skia::{Color, FillRule, FilterQuality, Paint, Pixmap, PixmapPaint, PixmapRef, Rect, Transform};

use crate::police::Polices;

const LOGO: &[u8] = include_bytes!("../../../ressources/logo-verdiercam-60.png");

/// L'image de la vidéo commence à x = 634.
pub const BANDEAU: i64 = 634;
/// Air minimal entre un texte et l'image.
pub const MARGE_DROITE: i64 = 20;

pub const FOND: (u8, u8, u8) = (0x1E, 0x1E, 0x1E);
pub const ORANGE: (u8, u8, u8) = (0xFF, 0x6D, 0x00);
pub const GRIS_CLAIR: (u8, u8, u8) = (0xE6, 0xE6, 0xE6);
pub const BLANC: (u8, u8, u8) = (0xFF, 0xFF, 0xFF);

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub gras: bool,
    pub corps: f32,
    pub couleur: (u8, u8, u8),
    pub x: i64,
    /// Ligne de base.
    pub y: i64,
}

impl Style {
    pub fn largeur_max(&self) -> i64 {
        BANDEAU - MARGE_DROITE - self.x
    }
}

/// Les cinq textes de la vignette, dans l'ordre des champs du Monteur 1.
pub const STYLES: [(&str, Style); 5] = [
    ("titre", Style { gras: true, corps: 112.0, couleur: BLANC, x: 62, y: 285 }),
    ("sous_titre", Style { gras: false, corps: 42.0, couleur: GRIS_CLAIR, x: 70, y: 380 }),
    ("ligne1", Style { gras: true, corps: 36.0, couleur: ORANGE, x: 70, y: 492 }),
    ("ligne2", Style { gras: true, corps: 36.0, couleur: ORANGE, x: 70, y: 552 }),
    ("ligne3", Style { gras: true, corps: 36.0, couleur: ORANGE, x: 70, y: 610 }),
];

fn texte_de<'f>(fiche: &'f Fiche, champ: &str) -> &'f str {
    match champ {
        "titre" => &fiche.titre,
        "sous_titre" => &fiche.sous_titre,
        "ligne1" => &fiche.ligne1,
        "ligne2" => &fiche.ligne2,
        _ => &fiche.ligne3,
    }
}

fn plein((r, g, b): (u8, u8, u8), anticrenele: bool) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, 255);
    p.anti_alias = anticrenele;
    p
}

/// La largeur en pixels de chaque texte (0 s'il est vide).
pub fn largeurs(fiche: &Fiche, polices: &Polices) -> Vec<(&'static str, f32)> {
    STYLES
        .iter()
        .map(|(champ, s)| {
            let t = texte_de(fiche, champ);
            (*champ, if t.trim().is_empty() { 0.0 } else { polices.de(s.gras).avance(t, s.corps) })
        })
        .collect()
}

/// Les textes qui mordent sur l'image, avec leurs pixels de trop.
pub fn debordements(fiche: &Fiche, polices: &Polices) -> Vec<(&'static str, i64)> {
    let styles: std::collections::HashMap<_, _> = STYLES.into_iter().collect();
    largeurs(fiche, polices)
        .into_iter()
        .filter_map(|(c, l)| {
            let trop = l.ceil() as i64 - styles[c].largeur_max();
            (trop > 0).then_some((c, trop))
        })
        .collect()
}

/// La vignette 1280 × 720. `image` : l'image pleine de la vidéo ; `cadre` : la part à mettre à
/// droite, en pixels de cette image (sans cadre, toute l'image est mise à 646 × 720).
pub fn vignette(fiche: &Fiche, image: Option<PixmapRef>, cadre: Option<[i64; 4]>, polices: &Polices) -> Pixmap {
    let mut v = Pixmap::new(VIGNETTE_L as u32, VIGNETTE_H as u32).expect("taille fixe");
    v.fill(Color::from_rgba8(FOND.0, FOND.1, FOND.2, 255));
    if let Some(image) = image {
        let [x, y, l, h] = cadre.unwrap_or([0, 0, image.width() as i64, image.height() as i64]);
        let tr = Transform::from_translate(BANDEAU as f32, 0.0)
            .pre_scale(IMAGE_L as f32 / l as f32, IMAGE_H as f32 / h as f32)
            .pre_translate(-x as f32, -y as f32);
        let peinture = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
        let mut m = tiny_skia::Mask::new(v.width(), v.height()).expect("taille fixe");
        let droite = Rect::from_xywh(BANDEAU as f32, 0.0, IMAGE_L as f32, IMAGE_H as f32).expect("rectangle");
        m.fill_path(&tiny_skia::PathBuilder::from_rect(droite), FillRule::Winding, false, Transform::identity());
        v.draw_pixmap(0, 0, image, &peinture, tr, Some(&m));
    }
    // Le filet orange du bord gauche (pixels 0 à 11), net.
    v.fill_rect(Rect::from_xywh(0.0, 0.0, 12.0, VIGNETTE_H as f32).unwrap(), &plein(ORANGE, false), Transform::identity(), None);
    if let Some(logo) = Pixmap::decode_png(LOGO).ok() {
        v.draw_pixmap(76, 70, logo.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    let texte = |v: &mut Pixmap, t: &str, gras: bool, corps: f32, c, x: i64, y: i64| {
        if let Some(chemin) = polices.de(gras).chemin(t, corps, x as f32, y as f32) {
            v.fill_path(&chemin, &plein(c, true), FillRule::Winding, Transform::identity(), None);
        }
    };
    texte(&mut v, "VerdierCAM", true, 40.0, ORANGE, 160, 113);
    for (champ, s) in STYLES {
        if champ == "ligne1" {
            // Le trait orange, entre le sous-titre et les trois lignes (pixels 70 à 150, 430 à 436).
            v.fill_rect(Rect::from_xywh(70.0, 430.0, 81.0, 7.0).unwrap(), &plein(ORANGE, false), Transform::identity(), None);
        }
        let t = texte_de(fiche, champ);
        if !t.trim().is_empty() {
            texte(&mut v, t, s.gras, s.corps, s.couleur, s.x, s.y);
        }
    }
    v
}
