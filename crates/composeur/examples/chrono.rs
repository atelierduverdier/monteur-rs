//! Mesure : combien coûte une image montée dessinée au processeur, à 1920 × 1200 ? Rejoue les
//! instants de `tests/donnees/reference.json`, 50 fois chacun, sur un fil puis sur tous.
use std::time::Instant;

use composeur::dessin::{Compositeur, R};
use composeur::police::Polices;
use noyau::montage::Montage;
use tiny_skia::Pixmap;

fn main() {
    let d = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/donnees");
    let r: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{d}/reference.json")).unwrap()).unwrap();
    let m = Montage::depuis_valeur(r["montage"].clone()).unwrap();
    let polices = Polices::charger();
    let c = Compositeur::new(&m, &polices);
    let instants: Vec<f64> = r["instants"].as_array().unwrap().iter().map(|t| t.as_f64().unwrap()).collect();
    let sources: Vec<Pixmap> = (0..instants.len()).map(|i| Pixmap::load_png(format!("{d}/source-{i}.png")).unwrap()).collect();
    let (l, h) = (sources[0].width(), sources[0].height());
    let cible = R::new(0.0, 0.0, l as f64, h as f64);
    let mut sortie = Pixmap::new(l, h).unwrap();
    let mut calque = Pixmap::new(l, h).unwrap();
    for (i, t) in instants.iter().enumerate() {
        let debut = Instant::now();
        for _ in 0..50 {
            c.peindre(&mut sortie.as_mut(), Some(sources[i].as_ref()), *t, cible, false);
        }
        let tout = debut.elapsed().as_secs_f64() * 1e3 / 50.0;
        let debut = Instant::now();
        for _ in 0..50 {
            calque.fill(tiny_skia::Color::TRANSPARENT);
            let source = c.vue(*t, l as f64, h as f64);
            c.peindre_calque(&mut calque.as_mut(), *t, cible, source, l as f64, h as f64);
        }
        let seul = debut.elapsed().as_secs_f64() * 1e3 / 50.0;
        println!("instant {i} : image entière {tout:.1} ms, calque seul {seul:.1} ms");
    }
    // Tous les fils, comme le rendu d'une prise.
    let fils = std::thread::available_parallelism().map_or(4, |n| n.get());
    let debut = Instant::now();
    let n = 40;
    std::thread::scope(|s| {
        for f in 0..fils {
            let (c, sources, instants) = (&c, &sources, &instants);
            s.spawn(move || {
                let mut sortie = Pixmap::new(l, h).unwrap();
                for k in 0..n {
                    let i = (f + k) % instants.len();
                    c.peindre(&mut sortie.as_mut(), Some(sources[i].as_ref()), instants[i], cible, false);
                }
            });
        }
    });
    let s = debut.elapsed().as_secs_f64();
    println!("{fils} fils : {} images en {s:.2} s → {:.0} images/s", fils * n, (fils * n) as f64 / s);
}
