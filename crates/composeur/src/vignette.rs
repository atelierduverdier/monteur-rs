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

/// Le corps d'un texte. Le titre : celui choisi, sinon 112 réduit juste assez pour tenir.
pub fn corps_de(fiche: &Fiche, champ: &str, s: &Style, polices: &Polices) -> f32 {
    if champ != "titre" {
        return s.corps;
    }
    if fiche.corps_titre > 0.0 {
        return fiche.corps_titre as f32;
    }
    let large = polices.de(s.gras).avance(&fiche.titre, s.corps);
    if large <= s.largeur_max() as f32 { s.corps } else { (s.corps * s.largeur_max() as f32 / large).floor() }
}

/// La largeur en pixels de chaque texte (0 s'il est vide).
pub fn largeurs(fiche: &Fiche, polices: &Polices) -> Vec<(&'static str, f32)> {
    STYLES
        .iter()
        .map(|(champ, s)| {
            let t = texte_de(fiche, champ);
            (*champ, if t.trim().is_empty() { 0.0 } else { polices.de(s.gras).avance(t, corps_de(fiche, champ, s, polices)) })
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

/// Le nom et le logo du haut du bandeau : VerdierCAM d'office, ou ceux d'une chaîne.
pub struct Marque {
    pub nom: String,
    pub logo: Option<Pixmap>,
}

impl Default for Marque {
    fn default() -> Self {
        Self { nom: "VerdierCAM".into(), logo: Pixmap::decode_png(LOGO).ok() }
    }
}

/// Le logo de la marque, ramené dans le carré de 60 px du logo de VerdierCAM.
fn poser_logo(v: &mut Pixmap, logo: &Pixmap) {
    let k = (60.0 / logo.width().max(logo.height()) as f32).min(1.0);
    let (l, h) = (logo.width() as f32 * k, logo.height() as f32 * k);
    let tr = Transform::from_translate(76.0 + (60.0 - l) / 2.0, 70.0 + (60.0 - h) / 2.0).pre_scale(k, k);
    let peinture = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
    v.draw_pixmap(0, 0, logo.as_ref(), &peinture, tr, None);
}

/// La vignette 1280 × 720 à la marque VerdierCAM. `image` : l'image pleine de la vidéo ; `cadre` : la part à mettre
/// à droite, en pixels de cette image (sans cadre, toute l'image est mise à 646 × 720).
pub fn vignette(fiche: &Fiche, image: Option<PixmapRef>, cadre: Option<[i64; 4]>, polices: &Polices) -> Pixmap {
    vignette_a_la_marque(fiche, image, cadre, polices, &Marque::default())
}

/// La vignette à la marque d'une chaîne : son nom et son logo à la place de ceux de VerdierCAM.
pub fn vignette_a_la_marque(fiche: &Fiche, image: Option<PixmapRef>, cadre: Option<[i64; 4]>, polices: &Polices, marque: &Marque) -> Pixmap {
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
    // Le logo de VerdierCAM se pose tel quel (60 × 60, au pixel près comme le Monteur 1) ; un autre se ramène à 60.
    if let Some(logo) = &marque.logo {
        if logo.width() == 60 && logo.height() == 60 {
            v.draw_pixmap(76, 70, logo.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        } else {
            poser_logo(&mut v, logo);
        }
    }
    let texte = |v: &mut Pixmap, t: &str, gras: bool, corps: f32, c, x: i64, y: i64| {
        if let Some(chemin) = polices.de(gras).chemin(t, corps, x as f32, y as f32) {
            v.fill_path(&chemin, &plein(c, true), FillRule::Winding, Transform::identity(), None);
        }
    };
    // Un nom trop long se resserre pour ne pas mordre sur l'image.
    let place = (BANDEAU - MARGE_DROITE - 160) as f32;
    let large = polices.de(true).avance(&marque.nom, 40.0);
    let corps = if large > place { (40.0 * place / large).floor() } else { 40.0 };
    texte(&mut v, &marque.nom, true, corps, ORANGE, 160, 113);
    for (champ, s) in STYLES {
        if champ == "ligne1" {
            // Le trait orange, entre le sous-titre et les trois lignes (pixels 70 à 150, 430 à 436).
            v.fill_rect(Rect::from_xywh(70.0, 430.0, 81.0, 7.0).unwrap(), &plein(ORANGE, false), Transform::identity(), None);
        }
        let t = texte_de(fiche, champ);
        if !t.trim().is_empty() {
            texte(&mut v, t, s.gras, corps_de(fiche, champ, &s, polices), s.couleur, s.x, s.y);
        }
    }
    v
}

#[cfg(test)]
mod tests_titre {
    use super::*;

    #[test]
    fn titre_qui_tient() {
        let p = Polices::charger();
        let titre = STYLES[0].1;
        let f = Fiche { titre: "FILETAGE".into(), ..Fiche::default() };
        assert_eq!(corps_de(&f, "titre", &titre, &p), 112.0);
        // Le titre de la remarque du 05/11 : 141 pixels de trop à 112.
        let mut f = Fiche { titre: "Dégagement".into(), ..Fiche::default() };
        let auto = corps_de(&f, "titre", &titre, &p);
        assert!(auto < 112.0 && debordements(&f, &p).is_empty(), "{auto}");
        // Choisi à la main, il est respecté, et le débordement se signale.
        f.corps_titre = 130.0;
        assert_eq!(corps_de(&f, "titre", &titre, &p), 130.0);
        assert!(!debordements(&f, &p).is_empty());
    }
}

/// Le carton « Fait avec VerdierCAM », 1280 × 720, sur le fond du montage (la vidéo s'y fond sans bord) : le logo,
/// la phrase, l'adresse.
pub fn carton_de_fin(polices: &Polices) -> Pixmap {
    let fond = u32::from_str_radix(noyau::FOND_MONTAGE.trim_start_matches("0x"), 16).unwrap_or(0x121214);
    let mut v = Pixmap::new(VIGNETTE_L as u32, VIGNETTE_H as u32).expect("taille fixe");
    v.fill(Color::from_rgba8((fond >> 16) as u8, (fond >> 8) as u8, fond as u8, 255));
    if let Ok(logo) = Pixmap::decode_png(LOGO) {
        // Le logo à trois fois sa taille, au-dessus de la phrase.
        let tr = Transform::from_translate(640.0 - 90.0, 170.0).pre_scale(3.0, 3.0);
        v.draw_pixmap(0, 0, logo.as_ref(), &PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() }, tr, None);
    }
    let centre = |v: &mut Pixmap, t: &str, gras: bool, corps: f32, c, y: f32| {
        let x = 640.0 - polices.de(gras).avance(t, corps) / 2.0;
        if let Some(chemin) = polices.de(gras).chemin(t, corps, x, y) {
            v.fill_path(&chemin, &plein(c, true), FillRule::Winding, Transform::identity(), None);
        }
    };
    centre(&mut v, "Fait avec VerdierCAM", true, 72.0, BLANC, 470.0);
    centre(&mut v, "verdiercam.fr", false, 40.0, ORANGE, 545.0);
    v
}

#[cfg(test)]
mod tests_marque {
    use super::*;

    fn polices() -> Polices {
        Polices::charger()
    }

    /// La marque d'office est celle de VerdierCAM : la vignette ne change pas d'un pixel.
    #[test]
    fn la_marque_d_office_ne_change_rien() {
        let p = polices();
        let f = Fiche { titre: "Essai".into(), ..Fiche::default() };
        assert_eq!(vignette(&f, None, None, &p).data(), vignette_a_la_marque(&f, None, None, &p, &Marque::default()).data());
    }

    /// Une autre chaîne : son nom à la place, son logo ramené dans le carré, le reste de la vignette identique.
    #[test]
    fn une_autre_chaine() {
        let p = polices();
        let f = Fiche { titre: "Essai".into(), ..Fiche::default() };
        let mut logo = Pixmap::new(200, 100).unwrap();
        logo.fill(Color::from_rgba8(0, 200, 0, 255));
        let a = vignette(&f, None, None, &p);
        let b = vignette_a_la_marque(&f, None, None, &p, &Marque { nom: "Mon atelier de bois tourné et sculpté".into(), logo: Some(logo) });
        let px = |v: &Pixmap, x: u32, y: u32| v.pixel(x, y).unwrap().demultiply();
        // Le logo vert, 60 × 30, centré dans le carré (76…136, 85…115).
        assert_eq!((px(&b, 100, 100).red(), px(&b, 100, 100).green()), (0, 200));
        assert_ne!(px(&b, 100, 75).green(), 200, "au-dessus du logo ramené, pas de vert");
        // Le nom s'est resserré : rien au-delà du bandeau.
        for x in BANDEAU as u32 - MARGE_DROITE as u32..BANDEAU as u32 {
            assert_eq!(px(&b, x, 100), px(&a, x, 100));
        }
        // Le titre n'a pas bougé.
        assert_eq!(px(&a, 100, 260), px(&b, 100, 260));
    }

    #[test]
    fn un_carton_de_fin() {
        let c = carton_de_fin(&polices());
        assert_eq!((c.width(), c.height()), (1280, 720));
        let px = |x: u32, y: u32| c.pixel(x, y).unwrap().demultiply();
        assert_eq!((px(5, 5).red(), px(5, 5).green(), px(5, 5).blue()), (0x12, 0x12, 0x14), "le fond du montage");
        let blancs = (300..980).filter(|&x| (430..480).any(|y| px(x, y).red() > 240 && px(x, y).blue() > 240)).count();
        assert!(blancs > 100, "la phrase est écrite : {blancs}");
    }
}
