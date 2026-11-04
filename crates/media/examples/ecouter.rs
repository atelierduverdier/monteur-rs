//! Mesure : joue une prise (en silence sauf `--entendre`) quelques secondes, saute, et compare
//! l'horloge du son à celle du système. `ecouter <prise> [--entendre]`.
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let p = std::path::PathBuf::from(&args[1]);
    let mut l = media::Lecteur::ouvrir(&p, 94.6, true).unwrap();
    if !args.iter().any(|a| a == "--entendre")
        && let Some(s) = &mut l.son
    {
        s.regler_volume(0.0);
    }
    for depart in [5.0, 60.0] {
        l.aller_a(depart);
        l.lire();
        let debut = Instant::now();
        std::thread::sleep(Duration::from_millis(300));
        let (p0, t0) = (l.position(), debut.elapsed().as_secs_f64());
        std::thread::sleep(Duration::from_secs(3));
        let (p1, t1) = (l.position(), debut.elapsed().as_secs_f64());
        let image = l.image().map(|i| i.t).unwrap_or(-1.0);
        println!(
            "depuis {depart} s : le son a avancé de {:.3} s en {:.3} s ; position {p1:.3}, image {image:.3}",
            p1 - p0,
            t1 - t0
        );
        l.arreter();
    }
}
