//! L'image d'une prise, décodée au processeur dans un fil à part (mesuré à l'étape 0 : 1 819
//! images/s jusqu'au RGBA, la carte graphique n'y gagne rien). Le fil décode en avance quelques
//! images ; un saut vide la file et repart de l'image clé d'avant.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

use ffmpeg_the_third as ff;
use ff::format::Pixel;
use ff::software::scaling;

/// Combien d'images le fil décode en avance.
const AVANCE: usize = 6;

/// Une image de la vidéo, en RGBA (non prémultiplié, opaque).
pub struct Image {
    /// En secondes de la vidéo.
    pub t: f64,
    pub largeur: u32,
    pub hauteur: u32,
    pub rgba: Vec<u8>,
    /// Le saut dont elle vient : une image d'avant le dernier saut est jetée.
    generation: u64,
}

enum Ordre {
    Aller { t: f64, generation: u64 },
    Fin,
}

pub struct Video {
    ordres: Sender<Ordre>,
    images: Receiver<Image>,
    fil: Option<JoinHandle<()>>,
    generation: u64,
    /// Les images reçues, pas encore montrées, dans l'ordre du temps.
    attente: std::collections::VecDeque<Image>,
    /// La dernière image montrée.
    courante: Option<Image>,
}

struct Decodeur {
    entree: ff::format::context::Input,
    flux: usize,
    base: f64,
    decodeur: ff::decoder::Video,
    trame: ff::frame::Video,
    echelle: Option<scaling::Context>,
    rgba: ff::frame::Video,
}

impl Decodeur {
    fn ouvrir(chemin: &Path) -> Result<Self, ff::Error> {
        let entree = ff::format::input(chemin)?;
        let flux = entree.streams().best(ff::media::Type::Video).ok_or(ff::Error::StreamNotFound)?;
        let (indice, base) = (flux.index(), f64::from(flux.time_base()));
        let mut ctx = ff::codec::Context::from_parameters(flux.parameters())?;
        ctx.set_threading(ff::codec::threading::Config {
            kind: ff::codec::threading::Type::Frame,
            count: 0,
            ..Default::default()
        });
        let decodeur = ctx.decoder().video()?;
        Ok(Self {
            entree,
            flux: indice,
            base,
            decodeur,
            trame: ff::frame::Video::empty(),
            echelle: None,
            rgba: ff::frame::Video::empty(),
        })
    }

    /// La seconde de la prochaine image décodée ; `None` à la fin.
    fn suivante(&mut self) -> Option<f64> {
        loop {
            if self.decodeur.receive_frame(&mut self.trame).is_ok() {
                return Some(self.trame.pts().or(self.trame.timestamp()).unwrap_or(0) as f64 * self.base);
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
                    return self
                        .decodeur
                        .receive_frame(&mut self.trame)
                        .ok()
                        .map(|_| self.trame.pts().unwrap_or(0) as f64 * self.base);
                }
            }
        }
    }

    fn en_image(&mut self, t: f64, generation: u64) -> Option<Image> {
        let (l, h) = (self.trame.width(), self.trame.height());
        let echelle = match &mut self.echelle {
            Some(e) => e,
            None => self
                .echelle
                .insert(scaling::Context::get(self.trame.format(), l, h, Pixel::RGBA, l, h, scaling::Flags::BILINEAR).ok()?),
        };
        echelle.run(&self.trame, &mut self.rgba).ok()?;
        // Les lignes de ffmpeg sont alignées : on les recopie serrées.
        let pas = self.rgba.stride(0);
        let donnees = self.rgba.data(0);
        let mut rgba = Vec::with_capacity((l * h * 4) as usize);
        for y in 0..h as usize {
            rgba.extend_from_slice(&donnees[y * pas..y * pas + l as usize * 4]);
        }
        Some(Image { t, largeur: l, hauteur: h, rgba, generation })
    }

    fn aller_a(&mut self, t: f64) {
        let ts = (t.max(0.0) * 1e6) as i64; // AV_TIME_BASE : la microseconde
        let _ = self.entree.seek(ts, ..=ts);
        self.decodeur.flush();
    }
}

impl Video {
    pub fn ouvrir(chemin: &Path) -> Result<Self, String> {
        // Les objets de ffmpeg restent dans le fil qui les a ouverts.
        let (ordres, recus) = mpsc::channel::<Ordre>();
        let (envoi, images) = mpsc::sync_channel::<Image>(AVANCE);
        let (ouvert, resultat) = mpsc::channel::<Result<(), String>>();
        let chemin = chemin.to_path_buf();
        let fil = std::thread::Builder::new()
            .name("décodage de l'image".into())
            .spawn(move || match Decodeur::ouvrir(&chemin) {
                Ok(mut d) => {
                    let _ = ouvert.send(Ok(()));
                    boucle(&mut d, recus, envoi);
                }
                Err(e) => {
                    let _ = ouvert.send(Err(format!("{} : {e}", chemin.display())));
                }
            })
            .map_err(|e| e.to_string())?;
        resultat.recv().map_err(|e| e.to_string())??;
        let mut v = Self { ordres, images, fil: Some(fil), generation: 0, attente: Default::default(), courante: None };
        v.aller_a(0.0);
        Ok(v)
    }

    /// Repart de `t` : les images d'avant sont oubliées, la prochaine montrée est celle de `t`.
    pub fn aller_a(&mut self, t: f64) {
        self.generation += 1;
        self.attente.clear();
        let _ = self.ordres.send(Ordre::Aller { t, generation: self.generation });
    }

    /// L'image à montrer à l'instant `t` : la dernière décodée dont le temps est passé. Les plus
    /// vieilles sont jetées. `None` tant que rien n'est arrivé depuis le dernier saut.
    pub fn image_a(&mut self, t: f64) -> Option<&Image> {
        // On ne prend dans la file que ce qu'il faut montrer, et au plus une image d'avance. La
        // file, bornée, retient alors le fil de décodage. Vider toute la file à chaque tour
        // empilait la vidéo entière en mémoire (9 Mo l'image, 1 800 images/s) : à l'arrêt, 4 Go
        // en 1,6 s ; chez Christophe le 04/11/2026, 32 Go et le terminal tué par systemd-oomd.
        loop {
            while self.attente.front().is_some_and(|i| i.t <= t + 0.017) {
                self.courante = self.attente.pop_front();
            }
            if !self.attente.is_empty() {
                break;
            }
            match self.images.try_recv() {
                Ok(i) if i.generation == self.generation => self.attente.push_back(i),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        // Juste après un saut, la première image arrivée se montre même si elle est un peu en
        // avance (l'image clé est souvent avant, jamais après).
        if self.courante.as_ref().is_none_or(|c| c.generation != self.generation)
            && let Some(premiere) = self.attente.pop_front()
        {
            self.courante = Some(premiere);
        }
        self.courante.as_ref().filter(|c| c.generation == self.generation)
    }

    /// Les images gardées hors de la file (pour le contrôle de la mémoire).
    pub fn en_attente(&self) -> usize {
        self.attente.len()
    }

    /// Attend (au plus `delai`) que l'image d'après le dernier saut soit là : pour les essais et
    /// pour une image arrêtée nette.
    pub fn attendre_image(&mut self, t: f64, delai: Duration) -> Option<&Image> {
        let fin = std::time::Instant::now() + delai;
        while std::time::Instant::now() < fin {
            if self.image_a(t).is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.image_a(t)
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.ordres.send(Ordre::Fin);
        // Débloque le fil s'il attend de la place dans la file.
        while self.images.try_recv().is_ok() {}
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}

fn boucle(d: &mut Decodeur, ordres: Receiver<Ordre>, envoi: SyncSender<Image>) {
    let mut generation = 0;
    let mut cible: Option<f64> = None;
    let mut fini = false;
    loop {
        // Un ordre arrivé passe avant tout.
        match ordres.try_recv() {
            Ok(Ordre::Aller { t, generation: g }) => {
                generation = g;
                d.aller_a(t);
                cible = Some(t);
                fini = false;
                continue;
            }
            Ok(Ordre::Fin) | Err(mpsc::TryRecvError::Disconnected) => return,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if fini {
            match ordres.recv_timeout(Duration::from_millis(20)) {
                Ok(Ordre::Aller { t, generation: g }) => {
                    generation = g;
                    d.aller_a(t);
                    cible = Some(t);
                    fini = false;
                }
                Ok(Ordre::Fin) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            continue;
        }
        let Some(t) = d.suivante() else {
            fini = true;
            continue;
        };
        // Après un saut, on décode sans convertir jusqu'à l'image voulue.
        if let Some(c) = cible {
            if t + 0.017 < c {
                continue;
            }
            cible = None;
        }
        let Some(mut image) = d.en_image(t, generation) else { continue };
        // La file pleine : on attend de la place, sans manquer un ordre.
        loop {
            match envoi.try_send(image) {
                Ok(()) => break,
                Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(i)) => {
                    image = i;
                    match ordres.recv_timeout(Duration::from_millis(4)) {
                        Ok(Ordre::Aller { t, generation: g }) => {
                            generation = g;
                            d.aller_a(t);
                            cible = Some(t);
                            fini = false;
                            break;
                        }
                        Ok(Ordre::Fin) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            }
        }
    }
}
