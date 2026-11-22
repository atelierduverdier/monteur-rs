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

/// Un WAV en 16 bits, écrit au fil de l'eau : l'entête porte des tailles provisoires, remises à la fin.
pub struct EcrivainWav {
    fichier: std::io::BufWriter<std::fs::File>,
    octets: u64,
}

impl EcrivainWav {
    pub fn creer(chemin: &std::path::Path, frequence: u32, canaux: u16) -> std::io::Result<Self> {
        use std::io::Write;
        let mut fichier = std::io::BufWriter::new(std::fs::File::create(chemin)?);
        let par_image = canaux as u32 * 2;
        fichier.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        fichier.write_all(&16u32.to_le_bytes())?;
        fichier.write_all(&1u16.to_le_bytes())?; // PCM
        fichier.write_all(&canaux.to_le_bytes())?;
        fichier.write_all(&frequence.to_le_bytes())?;
        fichier.write_all(&(frequence * par_image).to_le_bytes())?;
        fichier.write_all(&(par_image as u16).to_le_bytes())?;
        fichier.write_all(&16u16.to_le_bytes())?;
        fichier.write_all(b"data\0\0\0\0")?;
        Ok(Self { fichier, octets: 0 })
    }

    pub fn ecrire(&mut self, echantillons: &[i16]) -> std::io::Result<()> {
        use std::io::Write;
        for e in echantillons {
            self.fichier.write_all(&e.to_le_bytes())?;
        }
        self.octets += echantillons.len() as u64 * 2;
        Ok(())
    }

    /// Les vraies tailles dans l'entête. Au-delà de 4 Gio (6 h de stéréo à 48 kHz), elles plafonnent.
    pub fn finir(mut self) -> std::io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let data = self.octets.min(u32::MAX as u64 - 36) as u32;
        self.fichier.seek(SeekFrom::Start(4))?;
        self.fichier.write_all(&(data + 36).to_le_bytes())?;
        self.fichier.seek(SeekFrom::Start(40))?;
        self.fichier.write_all(&data.to_le_bytes())?;
        self.fichier.flush()
    }
}

/// Une piste son enregistrée pour une prise : le micro, ou le son du bureau (Windows seulement, par le mode
/// « loopback » de WASAPI). Les échantillons partent du fil de la carte son vers un fil qui écrit le WAV : le fil de
/// la carte ne touche jamais au disque.
pub struct PisteSon {
    flux: Option<cpal::Stream>,
    debut: Arc<Mutex<Option<Instant>>>,
    fil: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    pub chemin: std::path::PathBuf,
    pub canaux: u16,
}

/// Ce qu'une piste finie laisse : son WAV, l'instant de son premier échantillon, ses canaux.
pub struct PisteFinie {
    pub chemin: std::path::PathBuf,
    pub debut: Option<Instant>,
    pub canaux: u16,
}

impl PisteSon {
    /// Le micro par défaut. `None` sans micro.
    pub fn micro(chemin: &std::path::Path) -> Option<Self> {
        let entree = cpal::default_host().default_input_device()?;
        Self::lancer(entree, chemin, false)
    }

    /// Le son du bureau. Sous Windows, une sortie ouverte en entrée s'écoute (WASAPI) ; ailleurs, `None` : sous Linux,
    /// le « default » d'ALSA donnerait le micro à la place.
    pub fn bureau(chemin: &std::path::Path) -> Option<Self> {
        if !cfg!(windows) {
            return None;
        }
        let sortie = cpal::default_host().default_output_device()?;
        Self::lancer(sortie, chemin, true)
    }

    /// `sortie` : une sortie écoutée (le bureau) ; sa configuration de sortie est celle qu'on enregistre.
    fn lancer(appareil: cpal::Device, chemin: &std::path::Path, sortie: bool) -> Option<Self> {
        let config = if sortie { appareil.default_output_config() } else { appareil.default_input_config() }.ok()?;
        let canaux = config.channels();
        let mut wav = EcrivainWav::creer(chemin, config.sample_rate(), canaux).ok()?;
        let (envoi, recu) = std::sync::mpsc::channel::<Vec<i16>>();
        let debut = Arc::new(Mutex::new(None));
        let d = debut.clone();
        let rappel = move |donnees: &[f32], info: &cpal::InputCallbackInfo| {
            if let Ok(mut d) = d.lock()
                && d.is_none()
            {
                // L'instant du premier échantillon : celui de l'appel, moins ce que la carte a gardé.
                let ts = info.timestamp();
                *d = Some(Instant::now() - ts.callback.duration_since(ts.capture));
            }
            let _ = envoi.send(donnees.iter().map(|e| (e.clamp(-1.0, 1.0) * 32767.0) as i16).collect());
        };
        let flux = appareil.build_input_stream(config.into(), rappel, |e| eprintln!("enregistrement du son : {e}"), None).ok()?;
        flux.play().ok()?;
        let fil = std::thread::Builder::new()
            .name("écriture du son".into())
            .spawn(move || {
                for paquet in recu {
                    wav.ecrire(&paquet)?;
                }
                wav.finir()
            })
            .ok()?;
        Some(Self { flux: Some(flux), debut, fil: Some(fil), chemin: chemin.to_path_buf(), canaux })
    }

    /// Arrête la carte, attend que le WAV soit écrit en entier.
    pub fn finir(mut self) -> Result<PisteFinie, String> {
        // Le flux lâché, l'envoi part avec lui : le fil d'écriture voit la fin.
        drop(self.flux.take());
        let ecrit = self.fil.take().map(|f| f.join().map_err(|_| "le fil d'écriture du son s'est arrêté".to_string()));
        match ecrit {
            Some(Ok(Err(e))) => return Err(format!("{} : {e}", self.chemin.display())),
            Some(Err(e)) => return Err(e),
            _ => {}
        }
        let debut = self.debut.lock().ok().and_then(|d| *d);
        Ok(PisteFinie { chemin: self.chemin.clone(), debut, canaux: self.canaux })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_wav_lisible() {
        let d = std::env::temp_dir().join(format!("monteur-wav-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("essai.wav");
        let mut w = EcrivainWav::creer(&f, 48000, 2).unwrap();
        w.ecrire(&vec![1000i16; 2 * 4800]).unwrap();
        w.finir().unwrap();
        let o = std::fs::read(&f).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(o.len(), 44 + 2 * 4800 * 2);
        assert_eq!(&o[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(o[4..8].try_into().unwrap()) as usize, o.len() - 8);
        assert_eq!(u32::from_le_bytes(o[40..44].try_into().unwrap()) as usize, o.len() - 44);
        assert_eq!(u16::from_le_bytes(o[22..24].try_into().unwrap()), 2, "deux canaux");
        assert_eq!(u32::from_le_bytes(o[24..28].try_into().unwrap()), 48000);
    }

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
