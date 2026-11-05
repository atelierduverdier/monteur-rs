//! La vignette contre celle de MACHINE du 23/10/2026, faite par le Monteur 1 (ImageMagick) : le
//! bandeau (634 × 720, tout ce que dessine la charte) doit être le même.

use std::path::{Path, PathBuf};

use composeur::police::Polices;
use composeur::vignette::{self, vignette};
use noyau::fiches::Fiche;
use tiny_skia::{Pixmap, Rect};

fn donnees() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees")
}

fn machine() -> Fiche {
    Fiche {
        titre: "MACHINE".into(),
        sous_titre: "ajouter sa CNC GRBL".into(),
        ligne1: "Genmitsu 4040 PRO".into(),
        ligne2: "les réglages lus dans la carte".into(),
        ligne3: "le premier usinage".into(),
        ..Fiche::default()
    }
}

/// Points dont une couleur s'écarte de plus de 10 % (comme `magick compare -fuzz 10%`, en plus strict).
fn points_differents(a: &Pixmap, b: &Pixmap) -> usize {
    a.pixels()
        .iter()
        .zip(b.pixels())
        .filter(|(p, q)| {
            let (p, q) = (p.demultiply(), q.demultiply());
            p.red().abs_diff(q.red()).max(p.green().abs_diff(q.green())).max(p.blue().abs_diff(q.blue())) > 25
        })
        .count()
}

/// L'encre d'une bande de la vignette (ce qui s'écarte du fond #1E1E1E) : boîte englobante,
/// centre de gravité et quantité. C'est ce que la charte fixe — place, taille, graisse, couleur —
/// indépendamment de l'anticrénelage, qui diffère d'ImageMagick (FreeType cale les hauts des
/// lettres sur la grille des pixels, tiny-skia non).
fn encre(img: &Pixmap, y0: u32, y1: u32) -> ([u32; 4], (f64, f64), f64) {
    let (mut bx0, mut bx1, mut by0, mut by1) = (u32::MAX, 0, u32::MAX, 0);
    let (mut sx, mut sy, mut somme) = (0.0, 0.0, 0.0);
    for y in y0..y1 {
        for x in 12..img.width() { // sans le filet orange du bord, le même partout
            let p = img.pixel(x, y).unwrap().demultiply();
            let d = p.red().abs_diff(0x1E).max(p.green().abs_diff(0x1E)).max(p.blue().abs_diff(0x1E)) as f64 / 255.0;
            if d > 0.5 {
                (bx0, bx1, by0, by1) = (bx0.min(x), bx1.max(x), by0.min(y), by1.max(y));
            }
            sx += d * x as f64;
            sy += d * y as f64;
            somme += d;
        }
    }
    ([bx0, bx1, by0, by1], (sx / somme, sy / somme), somme)
}

/// Les bandes de la charte : logo et nom, titre, sous-titre, trait, trois lignes.
const BANDES: [(&str, u32, u32); 7] = [
    ("logo et VerdierCAM", 60, 140),
    ("titre", 180, 300),
    ("sous-titre", 340, 395),
    ("trait", 425, 440),
    ("ligne 1", 455, 505),
    ("ligne 2", 515, 565),
    ("ligne 3", 575, 625),
];

/// Les écarts de chaque bande ; `Err` dit la première qui sort de la charte.
///
/// Tolérances mesurées le 04/11/2026 contre ImageMagick : bords à 1 px (le calage des lettres
/// sur la grille par FreeType), centre à 0,2 px en hauteur, 0,2 px en largeur sauf la ligne 1
/// « Genmitsu 4040 PRO » (0,8 px : ImageMagick espace un peu autrement les chiffres), encre à 1,3 %.
fn comparer_bandes(a: &Pixmap, b: &Pixmap) -> Result<Vec<String>, String> {
    let mut lignes = Vec::new();
    for (nom, y0, y1) in BANDES {
        let (ba, ca, ea) = encre(a, y0, y1);
        let (bb, cb, eb) = encre(b, y0, y1);
        let bord = ba.iter().zip(bb).map(|(p, q)| p.abs_diff(q)).max().unwrap();
        let (dx, dy) = ((ca.0 - cb.0).abs(), (ca.1 - cb.1).abs());
        let quantite = (eb / ea - 1.0).abs();
        lignes.push(format!(
            "{nom} : bords à {bord} px, centre à ({:+.2}, {:+.2}) px, encre à {:+.2} %",
            cb.0 - ca.0,
            cb.1 - ca.1,
            (eb / ea - 1.0) * 100.0
        ));
        if bord > 1 || dx > 1.0 || dy > 0.3 || quantite > 0.02 {
            return Err(lignes.join("\n"));
        }
    }
    Ok(lignes)
}

fn bandeau(v: &Pixmap) -> Pixmap {
    v.clone_rect(tiny_skia::IntRect::from_xywh(0, 0, 634, 720).unwrap()).unwrap()
}

#[test]
fn bandeau_comme_la_vignette_machine() {
    let polices = Polices::charger();
    let v = vignette(&machine(), None, None, &polices);
    assert_eq!((v.width(), v.height()), (1280, 720));
    let b = bandeau(&v);
    let attendu = Pixmap::load_png(donnees().join("bandeau-machine-2026-10-23.png")).unwrap();
    let dossier = Path::new(env!("CARGO_TARGET_TMPDIR")).join("composeur");
    std::fs::create_dir_all(&dossier).unwrap();
    b.save_png(dossier.join("bandeau.png")).unwrap();
    println!("points à plus de 10 % (anticrénelage) : {}", points_differents(&b, &attendu));
    match comparer_bandes(&b, &attendu) {
        Ok(l) => l.iter().for_each(|l| println!("{l}")),
        Err(e) => panic!("hors charte : {e}"),
    }
}

/// Le contrôle voit-il un pixel de trop ? Le titre poussé d'un pixel, un corps de moins, l'orange
/// changé : chacun doit sortir de la charte.
#[test]
fn un_pixel_de_travers_se_voit() {
    let polices = Polices::charger();
    let juste = bandeau(&vignette(&machine(), None, None, &polices));
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let mut decale = Pixmap::new(634, 720).unwrap();
        decale.fill(tiny_skia::Color::from_rgba8(0x1E, 0x1E, 0x1E, 255));
        decale.draw_pixmap(dx, dy, juste.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        assert!(comparer_bandes(&decale, &juste).is_err(), "décalé de ({dx}, {dy})");
    }
    let mut f = machine();
    f.sous_titre = "ajouter sa CNC GRBL ".into(); // une espace de plus : rien ne bouge
    assert!(comparer_bandes(&bandeau(&vignette(&f, None, None, &polices)), &juste).is_ok());
    f.ligne2 = "les réglages lus dans la carte.".into();
    assert!(comparer_bandes(&bandeau(&vignette(&f, None, None, &polices)), &juste).is_err(), "un point de plus");
}

#[test]
fn image_et_cadre_a_droite() {
    let polices = Polices::charger();
    // Une image bleue avec un rectangle rouge : un cadre tout entier dedans donne une droite rouge.
    let mut image = Pixmap::new(1920, 1200).unwrap();
    image.fill(tiny_skia::Color::from_rgba8(0x20, 0x40, 0x80, 255));
    let mut rouge = tiny_skia::Paint::default();
    rouge.set_color_rgba8(0xC0, 0x30, 0x20, 255);
    image.fill_rect(Rect::from_xywh(400.0, 200.0, 600.0, 700.0).unwrap(), &rouge, tiny_skia::Transform::identity(), None);
    let v = vignette(&Fiche::default(), Some(image.as_ref()), Some([450, 250, 323, 360]), &polices);
    let p = v.pixel(634 + 323, 360).unwrap().demultiply();
    assert_eq!((p.red(), p.green(), p.blue()), (0xC0, 0x30, 0x20));
    // Sans cadre, toute l'image : le coin haut gauche de la droite est bleu.
    let v = vignette(&Fiche::default(), Some(image.as_ref()), None, &polices);
    let p = v.pixel(640, 5).unwrap().demultiply();
    assert_eq!((p.red(), p.green(), p.blue()), (0x20, 0x40, 0x80));
}

#[test]
fn texte_trop_long_mesure() {
    let polices = Polices::charger();
    let mut f = machine();
    f.titre = "MACHINE À COMMANDE".into();
    // En automatique il se réduit ; une taille choisie est respectée, et le débordement se dit.
    assert!(vignette::debordements(&f, &polices).is_empty());
    f.corps_titre = 112.0;
    let trop = vignette::debordements(&f, &polices);
    assert_eq!(trop.len(), 1);
    assert_eq!(trop[0].0, "titre");
    assert!(trop[0].1 > 100, "{trop:?}");
    assert!(vignette::debordements(&machine(), &polices).is_empty());
}
