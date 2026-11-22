//! Les petits sons du Monteur, sans programme à côté : les bips du décompte avant de filmer,
//! l'écoute d'un passage de la voix, le niveau du micro pendant qu'on filme. Tout passe par `cpal`,
//! comme la lecture : PipeWire (par ALSA) sous Linux, WASAPI sous Windows. Ils remplacent `paplay`
//! et `parec`, qui n'existent que là où il y a PulseAudio ou PipeWire.
//!
//! Un flux de `cpal` reste dans le fil qui l'a ouvert : ces objets ne quittent pas le leur.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

/// La crête la plus basse qu'on note, celle d'un échantillon de 16 bits à 1 : −90,3 dB.
const PLANCHER: f32 = 1.0 / 32768.0;

/// Une crête (0 à 1) en dB.
pub fn crete_db(crete: f32) -> f32 {
    20.0 * crete.max(PLANCHER).log10()
}

/// Le son en paires gauche-droite à la fréquence `vers` : un son mono va des deux côtés ; d'une
/// fréquence à l'autre, on passe en ligne droite d'un échantillon au suivant (assez pour écouter).
fn en_paires(source: &[f32], canaux: usize, de: u32, vers: u32) -> Vec<(f32, f32)> {
    let canaux = canaux.max(1);
    let paires: Vec<(f32, f32)> = source.chunks_exact(canaux).map(|i| (i[0], if canaux > 1 { i[1] } else { i[0] })).collect();
    if de == vers || de == 0 || vers == 0 || paires.is_empty() {
        return paires;
    }
    let dernier = paires.len() - 1;
    let n = (paires.len() as u64 * vers as u64 / de as u64) as usize;
    (0..n)
        .map(|k| {
            let x = k as f64 * de as f64 / vers as f64;
            let i = x as usize;
            let t = (x - i as f64) as f32;
            let (a, b) = (paires[i.min(dernier)], paires[(i + 1).min(dernier)]);
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
        .collect()
}

/// Un son joué une fois sur la sortie par défaut. Il s'arrête quand on le lâche.
pub struct Ecoute {
    _flux: cpal::Stream,
    fini: Arc<AtomicBool>,
}

impl Ecoute {
    /// `pcm` : entrelacé, s16le, `canaux` canaux à `frequence` Hz (ce que ffmpeg sort avec
    /// `-f s16le`). `None` sans sortie de son.
    pub fn lancer(pcm: &[u8], frequence: u32, canaux: usize) -> Option<Self> {
        let son: Vec<f32> = pcm.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]) as f32 / 32768.0).collect();
        Self::lancer_f32(&son, frequence, canaux)
    }

    fn lancer_f32(son: &[f32], frequence: u32, canaux: usize) -> Option<Self> {
        let sortie = cpal::default_host().default_output_device()?;
        let config = sortie.default_output_config().ok()?;
        let canaux_sortie = (config.channels() as usize).max(1);
        let paires = en_paires(son, canaux, frequence, config.sample_rate());
        let fini = Arc::new(AtomicBool::new(paires.is_empty()));
        let f = fini.clone();
        let mut lu = 0;
        let flux = sortie
            .build_output_stream(
                config.into(),
                move |donnees: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for image in donnees.chunks_mut(canaux_sortie) {
                        let (g, d) = paires.get(lu).copied().unwrap_or((0.0, 0.0));
                        lu += 1;
                        for (c, e) in image.iter_mut().enumerate() {
                            *e = if c % 2 == 0 { g } else { d };
                        }
                    }
                    if lu >= paires.len() {
                        f.store(true, Ordering::Relaxed);
                    }
                },
                |e| eprintln!("son : {e}"),
                None,
            )
            .ok()?;
        flux.play().ok()?;
        Some(Self { _flux: flux, fini })
    }

    /// Tout est parti vers la carte son.
    pub fn finie(&self) -> bool {
        self.fini.load(Ordering::Relaxed)
    }
}

/// Un bip : 150 ms, monté et éteint en douceur (une demi-sinusoïde), à −11 dB.
fn bip(son: &mut [f32], hauteur: f64, frequence: u32) {
    let n = (frequence as usize * 3 / 20).min(son.len());
    for (i, e) in son.iter_mut().take(n).enumerate() {
        let t = i as f64 / frequence as f64;
        let enveloppe = (std::f64::consts::PI * i as f64 / n as f64).sin();
        *e = ((2.0 * std::f64::consts::PI * hauteur * t).sin() * enveloppe * 9000.0 / 32768.0) as f32;
    }
}

/// Les bips du décompte, un par seconde, à ces hauteurs (Hz). Attend qu'ils soient joués : à
/// appeler dans un fil à part.
pub fn jouer_des_bips(hauteurs: &[f64]) {
    const F: u32 = 48000;
    if hauteurs.is_empty() {
        return;
    }
    let mut son = vec![0f32; (hauteurs.len() - 1) * F as usize + F as usize * 3 / 20];
    for (k, &h) in hauteurs.iter().enumerate() {
        bip(&mut son[k * F as usize..], h, F);
    }
    let Some(e) = Ecoute::lancer_f32(&son, F, 1) else { return };
    let limite = Instant::now() + Duration::from_secs(hauteurs.len() as u64 + 1);
    while !e.finie() && Instant::now() < limite {
        std::thread::sleep(Duration::from_millis(20));
    }
    // Le dernier tampon est encore dans la carte.
    std::thread::sleep(Duration::from_millis(150));
}

/// Le niveau du micro par défaut, lu à part de tout enregistrement : la crête en dB, qui monte tout
/// de suite et redescend de 2 dB toutes les 50 ms (l'œil suit), et le dernier instant où elle a
/// passé le seuil.
pub struct Micro {
    _flux: cpal::Stream,
    pic: Arc<AtomicU32>,
    dernier_son: Arc<Mutex<Instant>>,
}

impl Micro {
    /// `None` sans micro.
    pub fn lancer(seuil_db: f32) -> Option<Self> {
        let entree = cpal::default_host().default_input_device()?;
        let config = entree.default_input_config().ok()?;
        // 50 ms d'échantillons, tous canaux.
        let bloc = (config.sample_rate() as usize * config.channels() as usize / 20).max(1);
        let pic = Arc::new(AtomicU32::new(crete_db(0.0).to_bits()));
        let dernier_son = Arc::new(Mutex::new(Instant::now()));
        let suivre = || {
            let (p, d) = (pic.clone(), dernier_son.clone());
            let (mut crete, mut vus) = (0f32, 0usize);
            move |e: f32| {
                crete = crete.max(e.abs());
                vus += 1;
                if vus >= bloc {
                    let db = crete_db(crete);
                    let avant = f32::from_bits(p.load(Ordering::Relaxed));
                    p.store(db.max(avant - 2.0).to_bits(), Ordering::Relaxed);
                    if db > seuil_db
                        && let Ok(mut t) = d.lock()
                    {
                        *t = Instant::now();
                    }
                    (crete, vus) = (0.0, 0);
                }
            }
        };
        // En virgule flottante d'abord, comme la lecture ; en 16 bits si la carte n'en veut pas.
        let flux = ecouter::<f32>(&entree, config.clone().into(), suivre()).or_else(|| ecouter::<i16>(&entree, config.into(), suivre()))?;
        flux.play().ok()?;
        Some(Self { _flux: flux, pic, dernier_son })
    }

    /// La crête, en dB.
    pub fn niveau(&self) -> f32 {
        f32::from_bits(self.pic.load(Ordering::Relaxed))
    }

    /// Depuis quand le micro n'a rien donné au-dessus du seuil.
    pub fn muet_depuis(&self) -> Duration {
        self.dernier_son.lock().map(|t| t.elapsed()).unwrap_or_default()
    }
}

fn ecouter<T>(entree: &cpal::Device, config: cpal::StreamConfig, mut suivre: impl FnMut(f32) + Send + 'static) -> Option<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    entree
        .build_input_stream(
            config,
            move |donnees: &[T], _: &cpal::InputCallbackInfo| {
                for &e in donnees {
                    suivre(f32::from_sample_(e));
                }
            },
            |e| eprintln!("micro : {e}"),
            None,
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cretes() {
        assert_eq!(crete_db(0.0).round(), -90.0);
        assert_eq!(crete_db(1.0).round(), 0.0);
        assert_eq!(crete_db(0.5).round(), -6.0);
    }

    #[test]
    fn paires_et_frequences() {
        // Stéréo à la même fréquence : rien ne bouge.
        assert_eq!(en_paires(&[0.1, 0.2, 0.3, 0.4], 2, 48000, 48000), vec![(0.1, 0.2), (0.3, 0.4)]);
        // Mono : des deux côtés.
        assert_eq!(en_paires(&[0.5, -0.5], 1, 48000, 48000), vec![(0.5, 0.5), (-0.5, -0.5)]);
        // Le double de la fréquence : deux fois plus d'images, le milieu entre deux échantillons.
        let p = en_paires(&[0.0, 0.0, 1.0, 1.0], 2, 24000, 48000);
        assert_eq!(p.len(), 4);
        assert_eq!(p[1], (0.5, 0.5));
        // 48 kHz vers 44,1 kHz : la même durée.
        assert_eq!(en_paires(&vec![0.0; 2 * 48000], 2, 48000, 44100).len(), 44100);
    }

    #[test]
    fn un_bip_dure_150_ms_et_s_eteint() {
        let mut son = vec![0f32; 48000];
        bip(&mut son, 660.0, 48000);
        assert!(son[..7200].iter().any(|e| e.abs() > 0.2));
        assert!(son[7200..].iter().all(|&e| e == 0.0));
        assert!(son[0].abs() < 1e-6, "monte en douceur");
        assert!(son.iter().all(|e| e.abs() <= 9000.0 / 32768.0 + 1e-6));
    }
}
