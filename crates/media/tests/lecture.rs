//! Le lecteur sur une vraie prise (lue seulement) : une image demandée est la bonne, un saut
//! arrive vite, la lecture avance au rythme de l'horloge. Sans la prise sur ce poste, rien n'est
//! contrôlé (la prise n'est pas dans le dépôt).

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Les trois contrôles décodent la même vidéo : en même temps, ils se volent les cœurs et la
/// mesure de la lecture le paie (81 ms vus une fois, 17 ms seul). Ils passent l'un après l'autre.
static UN_A_LA_FOIS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn prise() -> Option<PathBuf> {
    let p = std::env::home_dir()?.join("Vidéos/Copies d'écran/2026-11-02 10-06-06.mp4");
    p.exists().then_some(p)
}

/// L'image que sort ffmpeg lui-même à `t` (en RGBA, même conversion).
fn image_ffmpeg(p: &PathBuf, t: f64) -> Vec<u8> {
    let sortie = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{t:.3}"), "-i"])
        .arg(p)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-sws_flags", "bilinear", "-"])
        .output()
        .unwrap();
    sortie.stdout
}

#[test]
fn images_et_sauts() {
    let _tour = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = prise() else { return };
    let mut v = media::video::Video::ouvrir(&p).unwrap();
    for t in [0.0, 12.345, 55.6, 21.2, 80.0] {
        let debut = Instant::now();
        v.aller_a(t);
        let image = v.attendre_image(t, Duration::from_secs(2)).expect("une image");
        let ms = debut.elapsed().as_secs_f64() * 1e3;
        let (it, l, h) = (image.t, image.largeur, image.hauteur);
        let attendu = image_ffmpeg(&p, t);
        assert_eq!(attendu.len(), image.rgba.len(), "taille");
        let ecart: f64 = image.rgba.iter().zip(&attendu).map(|(a, b)| a.abs_diff(*b) as f64).sum::<f64>() / attendu.len() as f64;
        println!("saut à {t:6.3} s : image de {it:6.3} s ({l}×{h}) en {ms:5.1} ms, écart à ffmpeg {ecart:.3}/255");
        assert!((it - t).abs() < 1.0 / 30.0 + 1e-3, "image de {it} pour {t}");
        assert!(ecart < 1.0, "écart {ecart}");
        assert!(ms < 400.0, "saut en {ms} ms");
    }
}

#[test]
fn lecture_au_rythme_de_l_horloge() {
    let _tour = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = prise() else { return };
    // Sans son : ce contrôle ne fait pas de bruit, l'horloge est celle du système.
    let mut l = media::Lecteur::ouvrir(&p, 94.6, false).unwrap();
    l.aller_a(10.0);
    l.lire();
    let debut = Instant::now();
    let mut vues = 0;
    let mut pire: f64 = 0.0;
    while debut.elapsed() < Duration::from_secs(2) {
        let pos = l.position();
        if let Some(i) = l.image() {
            pire = pire.max((i.t - pos).abs());
            vues += 1;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let pos = l.position();
    println!("2 s de lecture : position {pos:.3} s, {vues} images vues, pire écart image/horloge {:.0} ms", pire * 1e3);
    assert!((pos - 12.0).abs() < 0.05);
    assert!(pire < 0.08, "l'image traîne de {pire} s");
    l.arreter();
    let arret = l.position();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(l.position(), arret, "arrêtée, l'horloge ne bouge plus");
}

#[test]
fn forme_du_son() {
    let _tour = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(p) = prise() else { return };
    let debut = Instant::now();
    let pics = media::pics_du_son(&p, 50).unwrap();
    println!("{} pics en {:.0} ms", pics.len(), debut.elapsed().as_secs_f64() * 1e3);
    assert!((pics.len() as f64 - 94.6 * 50.0).abs() < 60.0);
    assert!(pics.iter().any(|&x| x > 0.1) && pics.iter().all(|&x| (0.0..=1.0).contains(&x)));
}

/// La mémoire reste bornée, à l'arrêt comme en lecture : jamais plus d'une image gardée hors de la
/// file. Sans borne, toute la vidéo s'empilait (4 Go en 1,6 s à l'arrêt, le 04/11/2026).
#[test]
fn memoire_bornee() {
    let Some(p) = prise() else { return };
    let _tour = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let mut l = media::Lecteur::ouvrir(&p, 94.6, false).unwrap();
    l.aller_a(10.0);
    let mut pire = 0;
    // À l'arrêt d'abord (la position ne bouge pas), puis en lecture.
    for lecture in [false, true] {
        if lecture {
            l.lire();
        }
        let debut = Instant::now();
        while debut.elapsed() < Duration::from_millis(1500) {
            let _ = l.image();
            pire = pire.max(l.video.en_attente());
            std::thread::sleep(Duration::from_millis(16));
        }
    }
    println!("au plus {pire} image(s) gardée(s) hors de la file");
    assert!(pire <= 1, "{pire} images gardées");
}

/// Un passage accéléré : l'image va quatre fois plus vite, un saut en sort.
#[test]
fn passage_accelere() {
    let Some(p) = prise() else { return };
    let _tour = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let mut l = media::Lecteur::ouvrir(&p, 94.6, false).unwrap();
    l.aller_a(10.0);
    l.lire();
    l.commencer_passage(4.0);
    let debut = Instant::now();
    let mut derniere = 0.0;
    while debut.elapsed() < Duration::from_secs(1) {
        if let Some(i) = l.image() {
            derniere = i.t;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    let pos = l.position();
    println!("1 s à ×4 : position {pos:.2} s, image {derniere:.2} s");
    assert!((pos - 14.0).abs() < 0.1, "position {pos}");
    assert!((derniere - 14.0).abs() < 0.2, "l'image suit : {derniere}");
    l.aller_a(pos);
    assert!(!l.en_passage(), "un saut sort du passage");
    let t = l.position();
    std::thread::sleep(Duration::from_millis(500));
    assert!((l.position() - t - 0.5).abs() < 0.08, "de nouveau à vitesse normale");
}
