//! Lire une prise : l'image et le son décodés en fond, l'horloge prise sur le son quand il y en a
//! un, les sauts de la tête de lecture.

pub mod son;
pub mod sons;
pub mod video;

use std::path::Path;
use std::time::Instant;

pub use video::Image;

pub struct Lecteur {
    pub video: video::Video,
    pub son: Option<son::Son>,
    /// Sans son : l'horloge du système, depuis `depart` (instant, seconde de la prise).
    depart: Option<(Instant, f64)>,
    arret: f64,
    en_lecture: bool,
    /// D'où la lecture est partie : Stop y revient.
    parti_de: f64,
    /// La dernière fois que l'horloge a bougé : position, instant.
    vu: (f64, Instant),
    /// Un passage accéléré : d'où, et combien de fois plus vite. L'image va `v` fois plus vite
    /// que l'horloge ; le son, lui, continue à sa vitesse, comme au rendu, qui ne garde que le
    /// début du son du passage.
    passage: Option<(f64, f64)>,
    pub duree: f64,
}

impl Lecteur {
    /// `duree` : celle que donne ffprobe (le noyau la lit).
    pub fn ouvrir(chemin: &Path, duree: f64, avec_son: bool) -> Result<Self, String> {
        taire_les_avertissements();
        let video = video::Video::ouvrir(chemin)?;
        let son = if avec_son { son::Son::ouvrir(chemin)? } else { None };
        Ok(Self { video, son, depart: None, arret: 0.0, en_lecture: false, parti_de: 0.0, vu: (0.0, Instant::now()), passage: None, duree })
    }

    pub fn en_lecture(&self) -> bool {
        self.en_lecture
    }

    /// La seconde de la prise qu'on voit et qu'on entend.
    pub fn position(&self) -> f64 {
        let h = self.horloge();
        let t = self.passage.map_or(h, |(depart, v)| depart + (h - depart) * v);
        t.min(self.duree.max(0.0))
    }

    /// L'image passe `v` fois plus vite à partir d'ici, jusqu'au prochain saut.
    pub fn commencer_passage(&mut self, v: f64) {
        if self.en_lecture && v > 1.0 {
            self.passage = Some((self.horloge(), v));
        }
    }

    pub fn en_passage(&self) -> bool {
        self.passage.is_some()
    }

    fn horloge(&self) -> f64 {
        let t = if !self.en_lecture {
            self.arret
        } else if let Some(s) = &self.son {
            s.position()
        } else {
            self.depart.map_or(self.arret, |(i, t)| t + i.elapsed().as_secs_f64())
        };
        t.min(self.duree.max(0.0))
    }

    pub fn lire(&mut self) {
        if self.en_lecture {
            return;
        }
        if self.position() >= self.duree - 0.05 {
            self.aller_a(0.0);
        }
        self.en_lecture = true;
        self.parti_de = self.arret;
        self.vu = (self.arret, Instant::now());
        self.depart = Some((Instant::now(), self.arret));
        if let Some(s) = &mut self.son {
            s.lire();
        }
    }

    pub fn arreter(&mut self) {
        if !self.en_lecture {
            return;
        }
        self.arret = self.position();
        self.en_lecture = false;
        self.passage = None;
        if let Some(s) = &mut self.son {
            s.arreter();
            s.aller_a(self.arret);
        }
        self.video.aller_a(self.arret);
    }

    /// Arrête et revient d'où la lecture est partie : on réécoute le même passage en montant.
    pub fn stop(&mut self) {
        self.arreter();
        self.aller_a(self.parti_de);
    }

    pub fn basculer(&mut self) {
        if self.en_lecture { self.arreter() } else { self.lire() }
    }

    /// Saute à `t`, en lecture ou à l'arrêt.
    pub fn aller_a(&mut self, t: f64) {
        let t = t.clamp(0.0, self.duree.max(0.0));
        self.passage = None;
        self.arret = t;
        self.depart = Some((Instant::now(), t));
        self.video.aller_a(t);
        if let Some(s) = &mut self.son {
            s.aller_a(t);
        }
    }

    /// L'image à montrer maintenant ; à la fin de la prise, la lecture s'arrête.
    pub fn image(&mut self) -> Option<&Image> {
        let t = self.position();
        // Le son finit parfois quelques centièmes avant la durée de la prise : l'horloge se fige
        // juste avant la fin, et la lecture ne s'arrêterait jamais.
        if (t - self.vu.0).abs() > 0.05 {
            self.vu = (t, Instant::now());
        }
        let figee = t >= self.duree - 1.0 && self.vu.1.elapsed().as_secs_f64() > 0.3;
        if self.en_lecture && (t >= self.duree - 0.001 || figee) {
            self.arret = self.duree;
            self.en_lecture = false;
            if let Some(s) = &mut self.son {
                s.arreter();
            }
        }
        self.video.image_a(t)
    }
}

/// ffmpeg n'écrit plus que les erreurs. Ses avertissements sur les prises d'OBS (« UDTA parsing
/// failed retrying raw » : un bloc d'informations relu autrement, avec succès ; « Could not update
/// timestamps for discarded samples » : les échantillons de mise en route du son AAC) remplissaient
/// le terminal à chaque ouverture, sans rien d'utile. Les vraies erreurs remontent par `Result`.
pub fn taire_les_avertissements() {
    ffmpeg_the_third::util::log::set_level(ffmpeg_the_third::util::log::Level::Error);
}

/// La forme du son : `par_s` pics par seconde (le plus fort des deux voies, 0 à 1), pour la ligne
/// de temps. Décode toute la prise : à lancer dans un fil.
pub fn pics_du_son(chemin: &Path, par_s: u32) -> Result<Vec<f32>, String> {
    use ffmpeg_the_third as ff;
    taire_les_avertissements();
    let err = |e: ff::Error| format!("{} : {e}", chemin.display());
    let mut entree = ff::format::input(chemin).map_err(err)?;
    let Some(flux) = entree.streams().best(ff::media::Type::Audio) else { return Ok(Vec::new()) };
    let indice = flux.index();
    let mut decodeur = ff::codec::Context::from_parameters(flux.parameters()).map_err(err)?.decoder().audio().map_err(err)?;
    let mut rs = ff::software::resampling::Context::get2(
        decodeur.format(),
        decodeur.ch_layout().to_owned(),
        decodeur.rate(),
        ff::format::Sample::F32(ff::format::sample::Type::Packed),
        ff::ChannelLayout::STEREO,
        // Même fréquence : sans changement de fréquence, le rééchantillonneur ne garde rien en
        // réserve (à 8 000 Hz, 9 % des pics se perdaient).
        decodeur.rate(),
    )
    .map_err(err)?;
    let par_pic = (decodeur.rate() / par_s.max(1)) as usize;
    let (mut pics, mut courant, mut n) = (Vec::new(), 0.0f32, 0usize);
    let mut trame = ff::frame::Audio::empty();
    let mut converti = ff::frame::Audio::empty();
    let mut verser = |decodeur: &mut ff::decoder::Audio, pics: &mut Vec<f32>| {
        while decodeur.receive_frame(&mut trame).is_ok() {
            if rs.run(&trame, &mut converti).is_err() {
                continue;
            }
            for &(g, d) in &converti.plane::<(f32, f32)>(0)[..converti.samples()] {
                courant = courant.max(g.abs()).max(d.abs());
                n += 1;
                if n == par_pic {
                    pics.push(courant.min(1.0));
                    (courant, n) = (0.0, 0);
                }
            }
        }
    };
    for resultat in entree.packets() {
        let Ok((f, p)) = resultat else { break };
        if f.index() == indice && decodeur.send_packet(&p).is_ok() {
            verser(&mut decodeur, &mut pics);
        }
    }
    let _ = decodeur.send_eof();
    verser(&mut decodeur, &mut pics);
    Ok(pics)
}
