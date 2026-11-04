//! Essai 2 de l'étape 0 : décoder une vraie prise d'OBS, au processeur puis par la carte graphique
//! (VA-API), et mesurer ce que coûte chaque maillon jusqu'à une image RGBA prête pour la texture.
//!
//! `essai-video <prise.mp4> [secondes]` : décode les premières secondes (20 par défaut) dans les
//! deux modes, puis saute à 10 instants tirés de la prise, comme la tête de lecture qu'on glisse.
//! Ne lit que la prise, n'écrit rien.

use std::ptr;
use std::time::{Duration, Instant};

use ffmpeg_the_third as ff;
use ff::ffi;
use ff::format::Pixel;
use ff::software::scaling;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Logiciel,
    Vaapi,
}

impl Mode {
    fn nom(self) -> &'static str {
        match self {
            Mode::Logiciel => "processeur",
            Mode::Vaapi => "carte graphique (VA-API)",
        }
    }
}

/// Le décodeur demande le format de sortie : on prend celui de VA-API s'il est proposé.
unsafe extern "C" fn choisir_vaapi(
    _ctx: *mut ffi::AVCodecContext,
    mut formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    unsafe {
        while *formats != ffi::AVPixelFormat::NONE {
            if *formats == ffi::AVPixelFormat::VAAPI {
                return *formats;
            }
            formats = formats.add(1);
        }
    }
    eprintln!("VA-API n'est pas proposé pour ce flux");
    ffi::AVPixelFormat::NONE
}

struct Lecteur {
    entree: ff::format::context::Input,
    flux: usize,
    base: ff::Rational,
    decodeur: ff::decoder::Video,
    mode: Mode,
    trame: ff::frame::Video,
    copie: ff::frame::Video, // l'image ramenée de la carte (NV12)
    echelle: Option<scaling::Context>,
    rgba: ff::frame::Video,
    // temps cumulés par maillon
    t_decodage: Duration,
    t_rapatriement: Duration,
    t_conversion: Duration,
}

impl Lecteur {
    fn ouvrir(chemin: &str, mode: Mode) -> Result<Self, ff::Error> {
        let entree = ff::format::input(chemin)?;
        let flux = entree
            .streams()
            .best(ff::media::Type::Video)
            .ok_or(ff::Error::StreamNotFound)?;
        let (indice, base) = (flux.index(), flux.time_base());
        let mut ctx = ff::codec::Context::from_parameters(flux.parameters())?;
        match mode {
            Mode::Logiciel => ctx.set_threading(ff::codec::threading::Config {
                kind: ff::codec::threading::Type::Frame,
                count: 0, // autant de fils que de cœurs
                ..Default::default()
            }),
            Mode::Vaapi => unsafe {
                let mut appareil: *mut ffi::AVBufferRef = ptr::null_mut();
                let code = ffi::av_hwdevice_ctx_create(
                    &mut appareil,
                    ffi::AVHWDeviceType::VAAPI,
                    ptr::null(),
                    ptr::null_mut(),
                    0,
                );
                if code < 0 {
                    return Err(ff::Error::from(code));
                }
                let brut = ctx.as_mut_ptr();
                (*brut).hw_device_ctx = ffi::av_buffer_ref(appareil);
                (*brut).get_format = Some(choisir_vaapi);
                ffi::av_buffer_unref(&mut appareil);
            },
        }
        let decodeur = ctx.decoder().video()?;
        Ok(Self {
            entree,
            flux: indice,
            base,
            decodeur,
            mode,
            trame: ff::frame::Video::empty(),
            copie: ff::frame::Video::empty(),
            echelle: None,
            rgba: ff::frame::Video::empty(),
            t_decodage: Duration::ZERO,
            t_rapatriement: Duration::ZERO,
            t_conversion: Duration::ZERO,
        })
    }

    /// La seconde d'une trame, d'après son horodatage.
    fn seconde(&self, pts: i64) -> f64 {
        pts as f64 * f64::from(self.base)
    }

    /// Décode jusqu'à la prochaine image ; `None` à la fin de la prise.
    fn image_suivante(&mut self) -> Option<f64> {
        loop {
            let debut = Instant::now();
            if self.decodeur.receive_frame(&mut self.trame).is_ok() {
                self.t_decodage += debut.elapsed();
                let pts = self.trame.pts().unwrap_or(0);
                return Some(self.seconde(pts));
            }
            let mut paquet = None;
            for resultat in self.entree.packets() {
                let Ok((flux, p)) = resultat else { break };
                if flux.index() == self.flux {
                    paquet = Some(p);
                    break;
                }
            }
            let debut = Instant::now();
            match paquet {
                Some(p) => {
                    let _ = self.decodeur.send_packet(&p);
                }
                None => {
                    let _ = self.decodeur.send_eof();
                    if self.decodeur.receive_frame(&mut self.trame).is_err() {
                        return None;
                    }
                    self.t_decodage += debut.elapsed();
                    let pts = self.trame.pts().unwrap_or(0);
                    return Some(self.seconde(pts));
                }
            }
            self.t_decodage += debut.elapsed();
        }
    }

    /// Ramène l'image décodée en mémoire (si elle est sur la carte) et la passe en RGBA.
    fn en_rgba(&mut self) -> Result<(), ff::Error> {
        let source = if self.mode == Mode::Vaapi {
            let debut = Instant::now();
            unsafe {
                let code = ffi::av_hwframe_transfer_data(self.copie.as_mut_ptr(), self.trame.as_ptr(), 0);
                if code < 0 {
                    return Err(ff::Error::from(code));
                }
            }
            self.t_rapatriement += debut.elapsed();
            &self.copie
        } else {
            &self.trame
        };
        let debut = Instant::now();
        let echelle = match &mut self.echelle {
            Some(e) => e,
            None => self.echelle.insert(scaling::Context::get(
                source.format(),
                source.width(),
                source.height(),
                Pixel::RGBA,
                source.width(),
                source.height(),
                scaling::Flags::BILINEAR,
            )?),
        };
        echelle.run(source, &mut self.rgba)?;
        self.t_conversion += debut.elapsed();
        Ok(())
    }

    /// Saute à `seconde` : à l'image clé d'avant, puis décode jusqu'à l'image voulue.
    fn aller_a(&mut self, seconde: f64) -> Result<(u32, f64), ff::Error> {
        let ts = (seconde * f64::from(ff::rescale::TIME_BASE.invert())) as i64;
        self.entree.seek(ts, ..=ts)?;
        self.decodeur.flush();
        let mut decodees = 0;
        while let Some(s) = self.image_suivante() {
            decodees += 1;
            if s + 1e-3 >= seconde {
                self.en_rgba()?;
                return Ok((decodees, s));
            }
        }
        Ok((decodees, f64::NAN))
    }
}

fn essayer(chemin: &str, mode: Mode, secondes: f64) -> Result<(), ff::Error> {
    println!("\n== {} ==", mode.nom());
    let mut l = Lecteur::ouvrir(chemin, mode)?;
    let debut = Instant::now();
    let mut images = 0u32;
    while let Some(s) = l.image_suivante() {
        if s > secondes {
            break;
        }
        l.en_rgba()?;
        images += 1;
    }
    let total = debut.elapsed().as_secs_f64();
    let par = |d: Duration| d.as_secs_f64() * 1e3 / images as f64;
    println!(
        "{images} images ({}×{}, {:?}) en {total:.2} s → {:.0} images/s",
        l.trame.width(),
        l.trame.height(),
        if mode == Mode::Vaapi { l.copie.format() } else { l.trame.format() },
        images as f64 / total
    );
    println!(
        "par image : décodage {:.2} ms, rapatriement {:.2} ms, conversion RGBA {:.2} ms",
        par(l.t_decodage),
        par(l.t_rapatriement),
        par(l.t_conversion)
    );

    // Dix sauts, à des instants fixes de la prise (répétables d'un essai à l'autre).
    let duree = l.entree.duration() as f64 / f64::from(ff::rescale::TIME_BASE.invert());
    let mut pire = 0.0f64;
    let mut somme = 0.0;
    for k in 0..10 {
        let cible = duree * ((k as f64 * 0.618_034) % 1.0) * 0.95;
        let t = Instant::now();
        let (decodees, arrivee) = l.aller_a(cible)?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        pire = pire.max(ms);
        somme += ms;
        println!("  saut à {cible:7.2} s → image de {arrivee:7.2} s, {decodees:3} images décodées, {ms:6.1} ms");
    }
    println!("sauts : moyenne {:.1} ms, pire {pire:.1} ms", somme / 10.0);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(chemin) = args.get(1) else {
        eprintln!("usage : essai-video <prise.mp4> [secondes]");
        std::process::exit(2);
    };
    let secondes = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20.0);
    ff::init().expect("ffmpeg");
    ff::util::log::set_level(ff::util::log::Level::Error);
    let mut echec = false;
    for mode in [Mode::Logiciel, Mode::Vaapi] {
        if let Err(e) = essayer(chemin, mode, secondes) {
            eprintln!("{} : échec : {e}", mode.nom());
            echec = true;
        }
    }
    if echec {
        std::process::exit(1);
    }
}
