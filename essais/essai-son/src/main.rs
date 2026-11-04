//! Essai 3 de l'étape 0 : le son d'une prise par `cpal`, horloge maîtresse de la lecture.
//!
//! `essai-son <prise.mp4> [secondes] [--entendre]` : décode tout le son de la prise (AAC) en
//! flottants à la fréquence de la sortie, puis le joue pendant `secondes` (15 par défaut) et mesure :
//! le retard entre l'écriture d'un tampon et sa sortie au haut-parleur (ce dont l'image doit tenir
//! compte), la taille des tampons, et la dérive de l'horloge du son face à l'horloge du système.
//! Sans `--entendre`, il joue du silence : les mesures sont les mêmes, les oreilles tranquilles.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ffmpeg_the_third as ff;
use ff::ChannelLayout;
use ff::format::Sample;
use ff::format::sample::Type as Disposition;

/// Tout le son de la prise, entrelacé gauche-droite, à `frequence`.
fn decoder_son(chemin: &str, frequence: u32) -> Result<Vec<f32>, ff::Error> {
    let mut entree = ff::format::input(chemin)?;
    let flux = entree
        .streams()
        .best(ff::media::Type::Audio)
        .ok_or(ff::Error::StreamNotFound)?;
    let indice = flux.index();
    let mut decodeur = ff::codec::Context::from_parameters(flux.parameters())?.decoder().audio()?;
    let mut reechantillonneur = ff::software::resampling::Context::get2(
        decodeur.format(),
        decodeur.ch_layout().to_owned(),
        decodeur.rate(),
        Sample::F32(Disposition::Packed),
        ChannelLayout::STEREO,
        frequence,
    )?;
    let mut sortie = Vec::new();
    let mut trame = ff::frame::Audio::empty();
    let mut converti = ff::frame::Audio::empty();
    let mut verser = |decodeur: &mut ff::decoder::Audio, sortie: &mut Vec<f32>| -> Result<(), ff::Error> {
        while decodeur.receive_frame(&mut trame).is_ok() {
            reechantillonneur.run(&trame, &mut converti)?;
            let n = converti.samples() * 2;
            sortie.extend_from_slice(&converti.plane::<(f32, f32)>(0).iter().flat_map(|&(g, d)| [g, d]).collect::<Vec<_>>()[..n]);
        }
        Ok(())
    };
    for resultat in entree.packets() {
        let (flux, paquet) = resultat?;
        if flux.index() == indice {
            decodeur.send_packet(&paquet)?;
            verser(&mut decodeur, &mut sortie)?;
        }
    }
    decodeur.send_eof()?;
    verser(&mut decodeur, &mut sortie)?;
    Ok(sortie)
}

#[derive(Default)]
struct Releves {
    retards_ms: Vec<f64>,
    tampons: Vec<usize>,
    // (instant système de l'appel, images déjà livrées) : leur pente donne la vraie fréquence
    points: Vec<(Instant, u64)>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(chemin) = args.get(1) else {
        eprintln!("usage : essai-son <prise.mp4> [secondes] [--entendre]");
        std::process::exit(2);
    };
    let secondes: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15.0);
    let entendre = args.iter().any(|a| a == "--entendre");

    let hote = cpal::default_host();
    let sortie = hote.default_output_device().expect("aucune sortie son");
    let config = sortie.default_output_config().expect("pas de configuration de sortie");
    println!("hôte {:?}, sortie « {} »", hote.id(), sortie.id().map(|i| i.to_string()).unwrap_or_default());
    println!("configuration par défaut : {config:?}");
    let frequence = config.sample_rate();
    let canaux = config.channels() as usize;

    ff::init().expect("ffmpeg");
    let debut = Instant::now();
    let son = decoder_son(chemin, frequence).expect("décodage du son");
    let duree_son = son.len() as f64 / 2.0 / frequence as f64;
    println!(
        "son décodé : {duree_son:.1} s en {:.0} ms ({:.0} fois le temps réel)",
        debut.elapsed().as_secs_f64() * 1e3,
        duree_son / debut.elapsed().as_secs_f64()
    );

    let son = Arc::new(son);
    let joues = Arc::new(AtomicU64::new(0)); // images de son (paires gauche-droite) livrées
    let releves = Arc::new(Mutex::new(Releves::default()));
    let (son_cb, joues_cb, releves_cb) = (son.clone(), joues.clone(), releves.clone());
    let volume = if entendre { 0.5 } else { 0.0 };

    let flux = sortie
        .build_output_stream(
            config.into(),
            move |donnees: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let images = donnees.len() / canaux;
                let depart = joues_cb.fetch_add(images as u64, Ordering::Relaxed) as usize;
                for (i, image) in donnees.chunks_mut(canaux).enumerate() {
                    let j = (depart + i) * 2;
                    let (g, d) = if j + 1 < son_cb.len() { (son_cb[j], son_cb[j + 1]) } else { (0.0, 0.0) };
                    for (c, echantillon) in image.iter_mut().enumerate() {
                        *echantillon = volume * if c % 2 == 0 { g } else { d };
                    }
                }
                let t = info.timestamp();
                let retard = t.playback.duration_since(t.callback);
                if let Ok(mut r) = releves_cb.try_lock() {
                    r.retards_ms.push(retard.as_secs_f64() * 1e3);
                    r.tampons.push(images);
                    r.points.push((Instant::now(), depart as u64));
                }
            },
            |e| eprintln!("erreur du flux son : {e}"),
            None,
        )
        .expect("ouverture du flux son");

    flux.play().expect("lecture");
    let t0 = Instant::now();
    let mut premier: Option<(Instant, u64)> = None;
    // On attend que le flux soit lancé : la mesure de dérive part du premier tampon livré.
    while t0.elapsed() < Duration::from_secs_f64(secondes) {
        std::thread::sleep(Duration::from_millis(250));
        let n = joues.load(Ordering::Relaxed);
        if premier.is_none() && n > 0 {
            premier = Some((Instant::now(), n));
        }
    }
    let (t1, n1) = (Instant::now(), joues.load(Ordering::Relaxed));
    drop(flux);

    let r = releves.lock().unwrap();
    let mut retards = r.retards_ms.clone();
    retards.sort_by(f64::total_cmp);
    let moyenne = retards.iter().sum::<f64>() / retards.len() as f64;
    let tampon = r.tampons.iter().sum::<usize>() as f64 / r.tampons.len() as f64;
    println!(
        "{} tampons de {:.0} images en moyenne ({:.1} ms chacun)",
        r.tampons.len(),
        tampon,
        tampon / frequence as f64 * 1e3
    );
    println!(
        "retard tampon → haut-parleur : min {:.1} ms, médian {:.1} ms, moyen {moyenne:.1} ms, max {:.1} ms",
        retards[0],
        retards[retards.len() / 2],
        retards[retards.len() - 1]
    );
    // Pente des images livrées face au temps système, après une seconde de mise en route.
    let origine = r.points[0].0;
    let pts: Vec<(f64, f64)> = r
        .points
        .iter()
        .map(|&(t, n)| ((t - origine).as_secs_f64(), n as f64))
        .filter(|&(t, _)| t > 1.0)
        .collect();
    let k = pts.len() as f64;
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f64>() / k, pts.iter().map(|p| p.1).sum::<f64>() / k);
    let pente = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>()
        / pts.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let ecart_max = pts
        .iter()
        .map(|p| ((p.1 - my) - pente * (p.0 - mx)).abs() / frequence as f64 * 1e3)
        .fold(0.0, f64::max);
    println!(
        "fréquence réelle de la sortie : {pente:.1} images/s pour {frequence} annoncées ({:+.0} millionièmes) ; plus grand écart d'un appel à la droite : {ecart_max:.1} ms",
        (pente / frequence as f64 - 1.0) * 1e6
    );
    let _ = (premier, t1, n1);
}
