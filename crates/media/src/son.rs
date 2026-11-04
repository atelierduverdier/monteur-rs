//! Le son d'une prise, décodé en fond dans un anneau que `cpal` vide. L'horloge de la lecture est
//! celle du son (mesuré à l'étape 0 : la sortie dérive de 136 à 266 millionièmes face à l'horloge
//! du système, soit 80 à 160 ms sur 10 min) : l'image suit les échantillons réellement joués.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ffmpeg_the_third as ff;
use ff::ChannelLayout;
use ff::format::Sample;
use ff::format::sample::Type as Disposition;

/// Le fil garde tant de secondes de son d'avance.
const AVANCE_S: f64 = 0.5;

/// Ce que le fil de décodage et la sortie se partagent.
struct Anneau {
    /// Échantillons entrelacés gauche-droite, à la fréquence de la sortie.
    echantillons: VecDeque<f32>,
    /// La seconde de la prise du premier échantillon jamais versé depuis le dernier saut.
    debut: f64,
    /// Images (paires gauche-droite) données à la sortie depuis le dernier saut.
    joues: u64,
    /// Quand la sortie en a pris pour la dernière fois.
    dernier_appel: Option<Instant>,
    /// Un saut demandé, pas encore fait par le fil.
    saut: Option<f64>,
    /// Le fil a fini la prise.
    fini: bool,
    volume: f32,
}

pub struct Son {
    anneau: Arc<Mutex<Anneau>>,
    flux: cpal::Stream,
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
    pub frequence: u32,
    /// Retard entre l'écriture d'un tampon et sa sortie au haut-parleur (mesuré à l'étape 0 :
    /// 21,3 ms, constant). Lu sur les horodatages de `cpal`.
    retard: Arc<Mutex<f64>>,
    en_lecture: bool,
}

impl Son {
    /// `None` si la prise n'a pas de son ou qu'il n'y a pas de sortie.
    pub fn ouvrir(chemin: &Path) -> Result<Option<Self>, String> {
        let hote = cpal::default_host();
        let Some(sortie) = hote.default_output_device() else { return Ok(None) };
        let config = sortie.default_output_config().map_err(|e| e.to_string())?;
        let frequence = config.sample_rate();
        let canaux = config.channels() as usize;
        let anneau = Arc::new(Mutex::new(Anneau {
            echantillons: VecDeque::with_capacity((frequence as f64 * AVANCE_S * 2.5) as usize),
            debut: 0.0,
            joues: 0,
            dernier_appel: None,
            saut: None,
            fini: false,
            volume: 1.0,
        }));
        let retard = Arc::new(Mutex::new(0.021));
        let (a, r) = (anneau.clone(), retard.clone());
        let flux = sortie
            .build_output_stream(
                config.into(),
                move |donnees: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    donnees.fill(0.0);
                    let Ok(mut an) = a.try_lock() else { return };
                    let images = donnees.len() / canaux;
                    let dispo = an.echantillons.len() / 2;
                    let n = images.min(dispo);
                    let v = an.volume;
                    for image in donnees.chunks_mut(canaux).take(n) {
                        let (g, d) = (an.echantillons.pop_front().unwrap_or(0.0), an.echantillons.pop_front().unwrap_or(0.0));
                        for (c, e) in image.iter_mut().enumerate() {
                            *e = v * if c % 2 == 0 { g } else { d };
                        }
                    }
                    an.joues += n as u64;
                    an.dernier_appel = Some(Instant::now());
                    let ts = info.timestamp();
                    if let Ok(mut r) = r.try_lock() {
                        *r = ts.playback.duration_since(ts.callback).as_secs_f64();
                    }
                },
                |e| eprintln!("son : {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        let _ = flux.pause();
        let arret = Arc::new(AtomicBool::new(false));
        let (a, s) = (anneau.clone(), arret.clone());
        // Les objets de ffmpeg restent dans le fil qui les a ouverts.
        let (ouvert, resultat) = std::sync::mpsc::channel::<Result<bool, String>>();
        let chemin = chemin.to_path_buf();
        let fil = std::thread::Builder::new()
            .name("décodage du son".into())
            .spawn(move || match DecodeurSon::ouvrir(&chemin, frequence) {
                Ok(Some(d)) => {
                    let _ = ouvert.send(Ok(true));
                    boucle(d, a, s, frequence);
                }
                Ok(None) => {
                    let _ = ouvert.send(Ok(false));
                }
                Err(e) => {
                    let _ = ouvert.send(Err(format!("{} : {e}", chemin.display())));
                }
            })
            .map_err(|e| e.to_string())?;
        if !resultat.recv().map_err(|e| e.to_string())?? {
            let _ = fil.join();
            return Ok(None);
        }
        Ok(Some(Self { anneau, flux, arret, fil: Some(fil), frequence, retard, en_lecture: false }))
    }

    pub fn lire(&mut self) {
        if !self.en_lecture {
            if let Ok(mut a) = self.anneau.lock() {
                a.dernier_appel = None;
            }
            let _ = self.flux.play();
            self.en_lecture = true;
        }
    }

    pub fn arreter(&mut self) {
        if self.en_lecture {
            // L'horloge s'arrête où elle en est : on repart de là.
            let t = self.position();
            let _ = self.flux.pause();
            self.en_lecture = false;
            self.aller_a(t);
        }
    }

    pub fn aller_a(&mut self, t: f64) {
        if let Ok(mut a) = self.anneau.lock() {
            a.echantillons.clear();
            a.debut = t.max(0.0);
            a.joues = 0;
            a.dernier_appel = None;
            a.saut = Some(t.max(0.0));
            a.fini = false;
        }
    }

    pub fn regler_volume(&mut self, v: f32) {
        if let Ok(mut a) = self.anneau.lock() {
            a.volume = v.clamp(0.0, 2.0);
        }
    }

    /// La seconde de la prise qui sort du haut-parleur en ce moment.
    pub fn position(&self) -> f64 {
        let Ok(a) = self.anneau.lock() else { return 0.0 };
        let joue = a.joues as f64 / self.frequence as f64;
        if !self.en_lecture || a.joues == 0 {
            return a.debut + joue;
        }
        // Entre deux appels de la sortie, l'horloge avance d'elle-même (au plus un tampon).
        let depuis = a.dernier_appel.map_or(0.0, |d| d.elapsed().as_secs_f64()).min(0.05);
        let retard = self.retard.lock().map_or(0.021, |r| *r);
        (a.debut + joue + depuis - retard).max(a.debut)
    }

    /// Le son est-il arrivé au bout de la prise ?
    pub fn fini(&self) -> bool {
        self.anneau.lock().is_ok_and(|a| a.fini && a.echantillons.is_empty())
    }
}

impl Drop for Son {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        let _ = self.flux.pause();
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}

struct DecodeurSon {
    entree: ff::format::context::Input,
    flux: usize,
    base: f64,
    decodeur: ff::decoder::Audio,
    reechantillonneur: ff::software::resampling::Context,
    trame: ff::frame::Audio,
    converti: ff::frame::Audio,
    frequence: u32,
}

impl DecodeurSon {
    fn ouvrir(chemin: &Path, frequence: u32) -> Result<Option<Self>, ff::Error> {
        let entree = ff::format::input(chemin)?;
        let Some(flux) = entree.streams().best(ff::media::Type::Audio) else { return Ok(None) };
        let (indice, base) = (flux.index(), f64::from(flux.time_base()));
        let decodeur = ff::codec::Context::from_parameters(flux.parameters())?.decoder().audio()?;
        let reechantillonneur = ff::software::resampling::Context::get2(
            decodeur.format(),
            decodeur.ch_layout().to_owned(),
            decodeur.rate(),
            Sample::F32(Disposition::Packed),
            ChannelLayout::STEREO,
            frequence,
        )?;
        Ok(Some(Self {
            entree,
            flux: indice,
            base,
            decodeur,
            reechantillonneur,
            trame: ff::frame::Audio::empty(),
            converti: ff::frame::Audio::empty(),
            frequence,
        }))
    }

    fn aller_a(&mut self, t: f64) {
        let ts = (t.max(0.0) * 1e6) as i64;
        let _ = self.entree.seek(ts, ..=ts);
        self.decodeur.flush();
    }

    /// Une trame de sortie assez grande : `run` l'allouerait au nombre d'échantillons de l'entrée,
    /// trop peu quand la sortie est à une fréquence plus haute que la prise (44,1 → 48 kHz) — le
    /// reste attendrait dans le rééchantillonneur et le son prendrait du retard.
    fn preparer_sortie(&mut self) {
        let entree = self.decodeur.rate().max(1) as usize;
        let n = self.trame.samples() * self.frequence as usize / entree + 256;
        self.converti = ff::frame::Audio::new(Sample::F32(Disposition::Packed), n, ff::ChannelLayoutMask::STEREO);
    }

    /// Le prochain morceau de son : sa seconde de départ et ses échantillons entrelacés ; `None`
    /// à la fin.
    fn suivant(&mut self) -> Option<(f64, Vec<f32>)> {
        loop {
            if self.decodeur.receive_frame(&mut self.trame).is_ok() {
                let t = self.trame.pts().unwrap_or(0) as f64 * self.base;
                self.preparer_sortie();
                self.reechantillonneur.run(&self.trame, &mut self.converti).ok()?;
                let n = self.converti.samples();
                let paires = &self.converti.plane::<(f32, f32)>(0)[..n];
                return Some((t, paires.iter().flat_map(|&(g, d)| [g, d]).collect()));
            }
            let mut paquet = None;
            for resultat in self.entree.packets() {
                let Ok((flux, p)) = resultat else { break };
                if flux.index() == self.flux {
                    paquet = Some(p);
                    break;
                }
            }
            match paquet {
                Some(p) => {
                    let _ = self.decodeur.send_packet(&p);
                }
                None => {
                    let _ = self.decodeur.send_eof();
                    if self.decodeur.receive_frame(&mut self.trame).is_err() {
                        return None;
                    }
                    let t = self.trame.pts().unwrap_or(0) as f64 * self.base;
                    self.preparer_sortie();
                    self.reechantillonneur.run(&self.trame, &mut self.converti).ok()?;
                    let n = self.converti.samples();
                    let paires = &self.converti.plane::<(f32, f32)>(0)[..n];
                    return Some((t, paires.iter().flat_map(|&(g, d)| [g, d]).collect()));
                }
            }
        }
    }
}

fn boucle(mut d: DecodeurSon, anneau: Arc<Mutex<Anneau>>, arret: Arc<AtomicBool>, frequence: u32) {
    let plein = (frequence as f64 * AVANCE_S) as usize * 2;
    // Après un saut, les échantillons d'avant l'instant voulu sont jetés.
    let mut cible: Option<f64> = Some(0.0);
    let mut fini = false;
    while !arret.load(Ordering::Relaxed) {
        let saut = anneau.lock().ok().and_then(|mut a| a.saut.take());
        if let Some(t) = saut {
            d.aller_a(t);
            cible = Some(t);
            fini = false;
        }
        let place = anneau.lock().map_or(0, |a| plein.saturating_sub(a.echantillons.len()));
        if fini || place == 0 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        let Some((t, mut morceau)) = d.suivant() else {
            fini = true;
            if let Ok(mut a) = anneau.lock() {
                a.fini = true;
            }
            continue;
        };
        if let Some(c) = cible {
            let fin = t + morceau.len() as f64 / 2.0 / frequence as f64;
            if fin <= c {
                continue;
            }
            let jeter = (((c - t) * frequence as f64).max(0.0) as usize * 2).min(morceau.len());
            morceau.drain(..jeter);
            cible = None;
        }
        if let Ok(mut a) = anneau.lock() {
            // Un saut arrivé pendant le décodage : ce morceau est d'avant, on le laisse.
            if a.saut.is_none() {
                a.echantillons.extend(morceau);
            }
        }
    }
}
