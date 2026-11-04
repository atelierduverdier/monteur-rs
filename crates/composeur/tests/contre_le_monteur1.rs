//! Les images montées du composeur contre celles du Monteur 1 (QPainter), sur une vraie prise :
//! `outils/reference_composeur.py` a fait dessiner au Monteur 1 cinq instants (réglage cliqué et
//! zoomé, atelier pendant son bond, boîte qui s'ouvre, passage accéléré, raccourci). Les deux
//! moteurs n'ont pas le même anticrénelage : on mesure l'écart, point par point.

use std::path::{Path, PathBuf};

use composeur::dessin::{Compositeur, R};
use composeur::police::Polices;
use noyau::montage::Montage;
use serde_json::Value;
use tiny_skia::Pixmap;

fn donnees() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees")
}

/// (part des points qui s'écartent de plus de 10 % sur une couleur, écart moyen sur 255).
pub fn ecart(a: &Pixmap, b: &Pixmap) -> (f64, f64) {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let (mut loin, mut somme) = (0u64, 0u64);
    for (p, q) in a.pixels().iter().zip(b.pixels()) {
        let (p, q) = (p.demultiply(), q.demultiply());
        let d = [p.red().abs_diff(q.red()), p.green().abs_diff(q.green()), p.blue().abs_diff(q.blue())];
        let max = *d.iter().max().unwrap();
        if max > 25 {
            loin += 1;
        }
        somme += d.iter().map(|&x| x as u64).sum::<u64>();
    }
    let n = a.pixels().len() as f64;
    (loin as f64 / n, somme as f64 / n / 3.0)
}

#[test]
fn images_montees_comme_le_monteur1() {
    let r: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join("reference.json")).unwrap()).unwrap();
    let m = Montage::depuis_valeur(r["montage"].clone()).unwrap();
    let polices = Polices::charger();
    let compositeur = Compositeur::new(&m, &polices);
    let sortie_dossier = Path::new(env!("CARGO_TARGET_TMPDIR")).join("composeur");
    std::fs::create_dir_all(&sortie_dossier).unwrap();
    let mut pire = (0.0f64, 0.0f64);
    for (i, t) in r["instants"].as_array().unwrap().iter().enumerate() {
        let t = t.as_f64().unwrap();
        let source = Pixmap::load_png(donnees().join(format!("source-{i}.png"))).unwrap();
        let attendu = Pixmap::load_png(donnees().join(format!("attendu-{i}.png"))).unwrap();
        let mut sortie = Pixmap::new(source.width(), source.height()).unwrap();
        let cible = R::new(0.0, 0.0, source.width() as f64, source.height() as f64);
        compositeur.peindre(&mut sortie.as_mut(), Some(source.as_ref()), t, cible, false);
        sortie.save_png(sortie_dossier.join(format!("rendu-{i}.png"))).unwrap();
        let (loin, moyen) = ecart(&sortie, &attendu);
        let (loin_source, _) = ecart(&source, &attendu);
        println!(
            "instant {i} ({t} s) : {:.3} % des points à plus de 10 % (la source seule : {:.3} %), écart moyen {moyen:.3}/255",
            loin * 100.0,
            loin_source * 100.0
        );
        pire = (pire.0.max(loin), pire.1.max(moyen));
    }
    println!("images dans {}", sortie_dossier.display());
    // Mesuré le 04/11/2026 : 0,018 % des points au plus quand tout est juste ; chacun des
    // sabotages essayés (rebond, durée du clic, marge des étiquettes) en met 0,049 % ou plus.
    assert!(pire.0 < 0.0003 && pire.1 < 1.0, "pire écart {pire:?}");
}

/// L'aperçu peint le calque à part (la carte graphique pose l'image zoomée dessous) : le résultat
/// doit être celui de l'image peinte d'un coup, à l'arrondi près.
#[test]
fn calque_a_part_comme_image_entiere() {
    let r: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join("reference.json")).unwrap()).unwrap();
    let m = Montage::depuis_valeur(r["montage"].clone()).unwrap();
    let polices = Polices::charger();
    let c = Compositeur::new(&m, &polices);
    let t = r["instants"][0].as_f64().unwrap();
    let source = Pixmap::load_png(donnees().join("source-0.png")).unwrap();
    let (l, h) = (source.width(), source.height());
    let cible = R::new(0.0, 0.0, l as f64, h as f64);
    let mut direct = Pixmap::new(l, h).unwrap();
    c.peindre(&mut direct.as_mut(), Some(source.as_ref()), t, cible, false);
    let vue = c.vue(t, l as f64, h as f64);
    let mut image = Pixmap::new(l, h).unwrap();
    composeur::dessin::peindre_image(&mut image.as_mut(), source.as_ref(), cible, vue);
    let mut calque = Pixmap::new(l, h).unwrap();
    c.peindre_calque(&mut calque.as_mut(), t, cible, vue, l as f64, h as f64);
    image.draw_pixmap(0, 0, calque.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
    let pire = direct
        .pixels()
        .iter()
        .zip(image.pixels())
        .map(|(p, q)| p.red().abs_diff(q.red()).max(p.green().abs_diff(q.green())).max(p.blue().abs_diff(q.blue())))
        .max()
        .unwrap();
    println!("calque à part : pire écart {pire}/255");
    assert!(pire <= 2, "pire écart {pire}");
}

/// La teinte moyenne du fond de la boîte du sous-titre (hors texte et hors bords arrondis).
fn fond_de_boite(img: &Pixmap, boite: R) -> f64 {
    let (mut somme, mut n) = (0.0, 0.0);
    for y in (boite.y.ceil() as u32 + 10)..(boite.y + boite.h) as u32 - 10 {
        for x in (boite.x.ceil() as u32 + 10)..(boite.x + boite.l) as u32 - 10 {
            let p = img.pixel(x, y).unwrap().demultiply();
            if p.red().max(p.green()).max(p.blue()) < 120 {
                somme += (p.red() as f64 + p.green() as f64 + p.blue() as f64) / 3.0;
                n += 1.0;
            }
        }
    }
    somme / n
}

/// L'encre blanche dans la boîte du sous-titre (son texte) : bords (gauche, droite, haut, bas)
/// et quantité.
fn encre_blanche(img: &Pixmap, boite: R) -> ([u32; 4], f64) {
    let mut b = [u32::MAX, 0, u32::MAX, 0];
    let mut somme = 0.0;
    for y in (boite.y.ceil() as u32 + 2)..(boite.y + boite.h) as u32 - 2 {
        for x in (boite.x.ceil() as u32 + 2)..(boite.x + boite.l) as u32 - 2 {
            let p = img.pixel(x, y).unwrap().demultiply();
            let w = ((p.red().min(p.green()).min(p.blue()) as f64 - 180.0) / 75.0).clamp(0.0, 1.0);
            if w > 0.5 {
                b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y)];
            }
            somme += w;
        }
    }
    (b, somme)
}

/// Les sous-titres incrustés, dont la phrase la plus longue de la prise, sur deux lignes.
#[test]
fn sous_titres_comme_le_monteur1() {
    let r: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join("reference.json")).unwrap()).unwrap();
    let m = Montage::depuis_valeur(r["montage_st"].clone()).unwrap();
    assert!(m.sous_titres_incrustes());
    let polices = Polices::charger();
    let c = Compositeur::new(&m, &polices);
    let dossier = Path::new(env!("CARGO_TARGET_TMPDIR")).join("composeur");
    std::fs::create_dir_all(&dossier).unwrap();
    for (k, paire) in r["instants_st"].as_array().unwrap().iter().enumerate() {
        let (i, t) = (paire[0].as_u64().unwrap(), paire[1].as_f64().unwrap());
        let source = Pixmap::load_png(donnees().join(format!("source-{i}.png"))).unwrap();
        let attendu = Pixmap::load_png(donnees().join(format!("attendu-st-{k}.png"))).unwrap();
        let mut sortie = Pixmap::new(source.width(), source.height()).unwrap();
        let cible = R::new(0.0, 0.0, source.width() as f64, source.height() as f64);
        c.peindre(&mut sortie.as_mut(), Some(source.as_ref()), t, cible, false);
        sortie.save_png(dossier.join(format!("rendu-st-{k}.png"))).unwrap();
        let (loin, moyen) = ecart(&sortie, &attendu);
        let boite = c.boite_sous_titre(t, cible).expect("un sous-titre à cet instant");
        let (ca, ea) = encre_blanche(&attendu, boite);
        let (cb, eb) = encre_blanche(&sortie, boite);
        println!(
            "sous-titre {k} ({t} s) : {:.3} % des points à plus de 10 %, écart moyen {moyen:.3}/255 ; \
             bords du texte {ca:?} chez Qt, {cb:?} ici, encre {:+.2} %",
            loin * 100.0,
            (eb / ea - 1.0) * 100.0
        );
        // Qt dessine le texte qu'il ne transforme pas avec les glyphes de FreeType, que le hinting
        // cale sur la grille et épaissit ; nous avec des chemins, comme Qt le texte agrandi.
        // Mesuré le 04/11/2026 : 0,26 % des points, 2,5 % d'encre en moins, inégalement selon les
        // lettres (le centre de l'encre en bouge de 1,4 px sur 1 360 : il ne dit rien) ; les bords
        // du texte, eux, restent au pixel, sauf le haut : le hinting monte les capitales jusqu'à
        // 2 px. Poser les lettres au pixel entier faisait pire.
        assert!(loin < 0.004 && moyen < 1.0, "sous-titre {k} : points");
        let (fa, fb) = (fond_de_boite(&attendu, boite), fond_de_boite(&sortie, boite));
        println!("  fond de la boîte : {fa:.2} chez Qt, {fb:.2} ici");
        // Mesuré le 04/11/2026 : 0,33/255 d'écart (arrondi du mélange) ; 0,70 au lieu de 0,72 en
        // met environ 4.
        assert!((fa - fb).abs() < 1.5, "sous-titre {k} : fond de la boîte");
        let tolerance = [1, 1, 2, 1]; // gauche, droite, haut, bas
        assert!(ca.iter().zip(cb).zip(tolerance).all(|((p, q), t)| p.abs_diff(q) <= t), "sous-titre {k} : bords du texte");
        assert!((eb / ea - 1.0).abs() < 0.04, "sous-titre {k} : encre");
    }
}
