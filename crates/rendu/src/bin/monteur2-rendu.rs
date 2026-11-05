//! `monteur2-rendu VIDÉO MONTAGE.json SORTIE.mp4` : le rendu sans fenêtre.

use std::path::Path;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 4 {
        eprintln!("usage : monteur2-rendu VIDÉO MONTAGE.json SORTIE.mp4");
        std::process::exit(2);
    }
    let Some(m) = noyau::montage::Montage::lire(Path::new(&a[2])) else {
        eprintln!("montage illisible : {}", a[2]);
        std::process::exit(1);
    };
    let debut = Instant::now();
    let suivi = rendu::Suivi::default();
    match rendu::rendre(Path::new(&a[1]), &m, Path::new(&a[3]), &suivi) {
        Ok(()) => {
            let s = debut.elapsed().as_secs_f64();
            let n = suivi.faites.load(std::sync::atomic::Ordering::Relaxed);
            println!("{n} images en {s:.1} s ({:.0} images/s)", n as f64 / s);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
