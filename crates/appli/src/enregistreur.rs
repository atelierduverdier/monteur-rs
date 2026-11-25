//! Filmer sans programme à côté (Windows) : une source donne des images BGRA au fil de l'eau (la fenêtre filmée), le
//! Monteur en fait une prise au nom daté d'OBS, comme `capture.rs` le fait sous Linux avec gpu-screen-recorder.
//!
//! - Les images partent dans ffmpeg par son entrée standard, au rythme fixe de 30 i/s : la dernière reçue est reprise
//!   tant qu'il n'en vient pas d'autre (une fenêtre immobile n'en donne pas). Le nombre d'images suit l'horloge : la
//!   vidéo dure ce qu'a duré la prise, et les sons restent calés.
//! - Chaque image se pose en haut à gauche d'un cadre fixe, celui de la première : les gestes de VerdierCAM sont
//!   rapportés au coin haut-gauche de sa fenêtre (`noyau::analyse::cadre`).
//! - Le son du bureau et le micro s'écrivent chacun dans un WAV, avec l'instant de leur premier échantillon. À l'arrêt,
//!   ffmpeg mêle le tout, chaque son calé sur la première image, le micro des deux côtés (comme `GRAPHE_SON`).
//! - Le décompte se joue AVANT de filmer : la fenêtre est déjà choisie, il n'y a pas de pause à faire.
//!
//! Tout ce fichier est commun à Linux et Windows : il s'essaie ici, avec une source d'images factice.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const IMAGES_PAR_S: u32 = 30;
/// Le décompte avant la première image.
const DECOMPTE: f64 = 3.0;
/// Sous ce niveau, le micro passe pour muet.
const SEUIL_MUET: f32 = -45.0;
/// La première image tarde au plus tant (une fenêtre fermée entre-temps, une capture refusée).
const PREMIERE_IMAGE_MAX: Duration = Duration::from_secs(10);
/// ffmpeg a tant pour finir sa vidéo une fois son entrée fermée ; au-delà, il est arrêté.
const FIN_DE_FFMPEG: Duration = Duration::from_secs(15);

pub struct Image {
    pub largeur: u32,
    pub hauteur: u32,
    /// BGRA, sans marge en bout de ligne.
    pub bgra: Vec<u8>,
}

/// Ce que la source partage avec le fil qui filme.
#[derive(Default)]
pub struct Boite {
    pub image: Option<Arc<Image>>,
    /// La fenêtre filmée s'est fermée.
    pub fermee: bool,
    pub erreur: Option<String>,
}

pub type Dernier = Arc<Mutex<Boite>>;

/// Une source d'images en marche ; `arreter` la coupe.
pub trait Source {
    fn arreter(&mut self);
}

/// Ce qui démarre la source, après le décompte : elle pose ses images dans la boîte.
pub type Demarrer = Box<dyn FnOnce(Dernier) -> Result<Box<dyn Source>, String>>;

pub enum Fin {
    /// La prise, et l'heure murale de son instant 0 (la webcam s'y cale).
    Prise(PathBuf, f64),
    Annulee,
    Erreur(String),
}

/// Pose l'image dans le cadre fixe `l × h`, en haut à gauche ; ce qui dépasse est coupé, ce qui manque est noir.
pub fn poser(image: &Image, cadre: &mut [u8], l: u32, h: u32) {
    let (lc, hc) = (image.largeur.min(l) as usize, image.hauteur.min(h) as usize);
    let (l, h, li) = (l as usize, h as usize, image.largeur as usize);
    for y in 0..h {
        let ligne = &mut cadre[y * l * 4..(y + 1) * l * 4];
        if y < hc && image.bgra.len() >= (y * li + lc) * 4 {
            ligne[..lc * 4].copy_from_slice(&image.bgra[y * li * 4..(y * li + lc) * 4]);
            ligne[lc * 4..].fill(0);
        } else {
            ligne.fill(0);
        }
    }
}

/// La zone cliente d'une fenêtre dans l'image filmée, `(x0, y0, x1, y1)` en pixels : l'image couvre le cadre visible
/// de la fenêtre (`cadre`, en coordonnées de l'écran), la zone cliente commence à `origine` et mesure `client`. `None`
/// si les mesures ne tiennent pas dans l'image : on garde alors l'image entière.
pub fn zone_cliente(cadre: (i32, i32), origine: (i32, i32), client: (i32, i32), image: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (x0, y0) = (origine.0 - cadre.0, origine.1 - cadre.1);
    if x0 < 0 || y0 < 0 || client.0 < 2 || client.1 < 2 {
        return None;
    }
    let (x0, y0) = (x0 as u32, y0 as u32);
    let (x1, y1) = ((x0 + client.0 as u32).min(image.0), (y0 + client.1 as u32).min(image.1));
    (x1 > x0 + 1 && y1 > y0 + 1).then_some((x0, y0, x1, y1))
}

/// Un encodeur H.264 : son nom pour ffmpeg et ses réglages. Ceux des cartes graphiques d'abord, x264 sinon.
pub(crate) fn encodeur() -> &'static [&'static str] {
    static CHOISI: OnceLock<&'static [&'static str]> = OnceLock::new();
    const CARTES: [&[&str]; 3] = [
        &["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "23", "-b:v", "0", "-pix_fmt", "yuv420p"],
        &["-c:v", "h264_amf", "-quality", "balanced", "-rc", "cqp", "-qp_i", "22", "-qp_p", "24", "-pix_fmt", "nv12"],
        &["-c:v", "h264_qsv", "-global_quality", "23", "-pix_fmt", "nv12"],
    ];
    const X264: &[&str] = &["-c:v", "libx264", "-preset", "veryfast", "-crf", "21", "-pix_fmt", "yuv420p"];
    CHOISI.get_or_init(|| {
        // Une image noire encodée pour de bon : le pilote dit s'il sait faire.
        let essaie = |reglages: &[&str]| {
            noyau::systeme::commande("ffmpeg")
                .args(["-v", "error", "-f", "lavfi", "-i", "color=black:s=320x240:d=0.2"])
                .args(reglages)
                .args(["-f", "null", "-"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        CARTES.into_iter().find(|r| essaie(r)).unwrap_or(X264)
    })
}

/// La commande qui encode les images reçues sur l'entrée standard.
fn commande_video(l: u32, h: u32, reglages: &[&str], sortie: &Path) -> std::process::Command {
    let mut c = noyau::systeme::commande("ffmpeg");
    c.args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgra", "-s", &format!("{l}x{h}"), "-framerate", &IMAGES_PAR_S.to_string(), "-i", "-"])
        .args(reglages)
        .args(["-g", &(2 * IMAGES_PAR_S).to_string()])
        .arg(sortie)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    c
}

/// La vidéo filmée : son fichier, l'heure de sa première image, son nombre d'images.
struct Video {
    chemin: PathBuf,
    premiere: SystemTime,
    nombre: u64,
}

/// Le fil qui filme : attend la première image, lance ffmpeg à sa taille, puis donne 30 images par seconde jusqu'à
/// l'arrêt. `Ok(None)` : arrêté avant la première image.
fn filmer(dernier: Dernier, arret: Arc<AtomicBool>, sortie: PathBuf, debut: Arc<Mutex<Option<Instant>>>) -> Result<Option<Video>, String> {
    let attente = Instant::now();
    let premiere = loop {
        if arret.load(Ordering::Relaxed) {
            return Ok(None);
        }
        {
            let b = dernier.lock().map_err(|_| "capture abîmée".to_string())?;
            if let Some(e) = &b.erreur {
                return Err(format!("Enregistrement impossible : {e}"));
            }
            if b.fermee {
                return Err("La fenêtre s'est fermée avant la première image.".into());
            }
            if let Some(i) = &b.image {
                break i.clone();
            }
        }
        if attente.elapsed() > PREMIERE_IMAGE_MAX {
            return Err("La fenêtre n'a donné aucune image.".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // H.264 en 4:2:0 veut des côtés pairs.
    let (l, h) = (premiere.largeur & !1, premiere.hauteur & !1);
    if l < 16 || h < 16 {
        return Err(format!("Fenêtre trop petite pour être filmée ({l} × {h})."));
    }
    let mut ffmpeg = commande_video(l, h, encodeur(), &sortie).spawn().map_err(|e| format!("ffmpeg n'a pas pu être lancé : {e}"))?;
    let derniere_ligne = Arc::new(Mutex::new(String::new()));
    if let Some(erreurs) = ffmpeg.stderr.take() {
        let ligne = derniere_ligne.clone();
        std::thread::spawn(move || {
            for l in BufReader::new(erreurs).lines().map_while(Result::ok) {
                if let Ok(mut d) = ligne.lock() {
                    *d = l;
                }
            }
        });
    }
    let mut entree = ffmpeg.stdin.take().ok_or("ffmpeg sans entrée")?;
    let mut cadre = vec![0u8; l as usize * h as usize * 4];
    let mut courante = premiere;
    poser(&courante, &mut cadre, l, h);
    let t0 = Instant::now();
    let heure = SystemTime::now();
    if let Ok(mut d) = debut.lock() {
        *d = Some(t0);
    }
    let pas = Duration::from_secs_f64(1.0 / IMAGES_PAR_S as f64);
    let mut nombre: u64 = 0;
    let mut panne = None;
    loop {
        let fin = arret.load(Ordering::Relaxed) || dernier.lock().map(|b| b.fermee).unwrap_or(true);
        let (nouvelle, erreur) = dernier.lock().map(|b| (b.image.clone(), b.erreur.clone())).unwrap_or_default();
        if let Some(e) = erreur {
            panne = Some(e);
            break;
        }
        if let Some(i) = nouvelle
            && !Arc::ptr_eq(&i, &courante)
        {
            courante = i;
            poser(&courante, &mut cadre, l, h);
        }
        // Autant d'images que l'horloge en demande : un encodeur en retard reçoit la même image plusieurs fois.
        let voulu = (t0.elapsed().as_secs_f64() * IMAGES_PAR_S as f64) as u64 + 1;
        let mut ecrites = 0;
        while nombre < voulu && ecrites < IMAGES_PAR_S {
            if let Err(e) = entree.write_all(&cadre) {
                panne = Some(format!("ffmpeg a lâché : {e}"));
                break;
            }
            nombre += 1;
            ecrites += 1;
        }
        if fin || panne.is_some() {
            break;
        }
        let prochaine = t0 + pas * (nombre as u32);
        std::thread::sleep(prochaine.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
    }
    drop(entree);
    let limite = Instant::now() + FIN_DE_FFMPEG;
    let etat = loop {
        match ffmpeg.try_wait() {
            Ok(Some(e)) => break Some(e),
            Ok(None) if Instant::now() < limite => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = ffmpeg.kill();
                let _ = ffmpeg.wait();
                break None;
            }
        }
    };
    let ligne = derniere_ligne.lock().map(|l| l.clone()).unwrap_or_default();
    match (etat, panne) {
        (_, Some(p)) => Err(if ligne.is_empty() { p } else { format!("{p} ({ligne})") }),
        (Some(e), None) if e.success() => Ok(Some(Video { chemin: sortie, premiere: heure, nombre })),
        (Some(_), None) => Err(format!("ffmpeg a échoué : {ligne}")),
        (None, None) => Err("ffmpeg ne finissait pas la vidéo : arrêté.".into()),
    }
}

/// Une piste son à mêler : son WAV, son décalage sur la première image (s, négatif s'il a commencé avant), ses canaux.
pub struct SonAMeler {
    pub chemin: PathBuf,
    pub decalage: f64,
    pub canaux: u16,
}

/// La commande qui fait la prise : la vidéo recopiée telle quelle, le bureau tel quel, le micro des deux côtés, le
/// tout coupé à la durée de la vidéo.
pub fn commande_melange(video: &Path, duree: f64, bureau: Option<&SonAMeler>, micro: Option<&SonAMeler>, sortie: &Path) -> Vec<String> {
    let s = |x: &str| x.to_string();
    let mut c = vec![s("ffmpeg"), s("-v"), s("error"), s("-y"), s("-i"), video.to_string_lossy().into_owned()];
    let entree = |son: &SonAMeler, c: &mut Vec<String>| {
        // Commencé avant la première image : on en saute le début ; après : on le retarde.
        if son.decalage < 0.0 {
            c.extend([s("-ss"), format!("{:.3}", -son.decalage)]);
        } else {
            c.extend([s("-itsoffset"), format!("{:.3}", son.decalage)]);
        }
        c.extend([s("-i"), son.chemin.to_string_lossy().into_owned()]);
    };
    let voix = |indice: usize, canaux: u16| {
        // Une voix d'un seul côté (la Scarlett) ou mono va des deux côtés.
        if canaux >= 2 { format!("[{indice}:a]pan=stereo|c0=c0+c1|c1=c0+c1[voix]") } else { format!("[{indice}:a]pan=stereo|c0=c0|c1=c0[voix]") }
    };
    let graphe = match (bureau, micro) {
        (Some(b), Some(m)) => {
            entree(b, &mut c);
            entree(m, &mut c);
            Some(format!("{};[1:a][voix]amix=inputs=2:normalize=0:duration=longest[son]", voix(2, m.canaux)))
        }
        (Some(b), None) => {
            entree(b, &mut c);
            Some(s("[1:a]anull[son]"))
        }
        (None, Some(m)) => {
            entree(m, &mut c);
            Some(voix(1, m.canaux).replace("[voix]", "[son]"))
        }
        (None, None) => None,
    };
    c.extend([s("-map"), s("0:v")]);
    if let Some(g) = graphe {
        c.extend([s("-filter_complex"), g, s("-map"), s("[son]"), s("-c:a"), s("aac"), s("-b:a"), s("192k")]);
    }
    c.extend([s("-c:v"), s("copy"), s("-t"), format!("{duree:.3}"), sortie.to_string_lossy().into_owned()]);
    c
}

/// Le premier nom libre à la façon d'OBS, à partir de l'instant de la première image : jamais par-dessus une prise.
fn nom_libre(dossier: &Path, heure: SystemTime) -> Option<PathBuf> {
    let instant = heure.duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    (0..60).map(|i| dossier.join(format!("{}.mp4", noyau::videos::nom_de_prise(instant + i as f64)))).find(|c| !c.exists())
}

/// La vidéo et ses sons deviennent une prise. Les fichiers de travail partent ; en cas d'échec, rien n'est perdu.
fn conclure(dossier: &Path, video: Result<Option<Video>, String>, sons: Vec<Result<media::sons::PisteFinie, String>>, t0: Option<Instant>, travail: &[PathBuf]) -> Fin {
    let nettoyer = || {
        for f in travail {
            let _ = std::fs::remove_file(f);
        }
    };
    let video = match video {
        Ok(Some(v)) => v,
        Ok(None) => {
            nettoyer();
            return Fin::Annulee;
        }
        Err(e) => {
            nettoyer();
            return Fin::Erreur(e);
        }
    };
    let Some(cible) = nom_libre(dossier, video.premiere) else {
        return Fin::Erreur(format!("Vidéo gardée sous {}", video.chemin.display()));
    };
    let duree = video.nombre as f64 / IMAGES_PAR_S as f64;
    // Chaque son, rapporté à la première image ; un son vide ou sans instant ne se mêle pas.
    let a_meler = |s: &Result<media::sons::PisteFinie, String>| {
        let s = s.as_ref().ok()?;
        let (d, t0) = (s.debut?, t0?);
        let decalage = if d >= t0 { d.duration_since(t0).as_secs_f64() } else { -t0.duration_since(d).as_secs_f64() };
        (std::fs::metadata(&s.chemin).is_ok_and(|m| m.len() > 44)).then(|| SonAMeler { chemin: s.chemin.clone(), decalage, canaux: s.canaux })
    };
    let (bureau, micro) = (sons.first().and_then(a_meler), sons.get(1).and_then(a_meler));
    let melange = cible.with_extension("son.partiel.mp4");
    let cmd = commande_melange(&video.chemin, duree, bureau.as_ref(), micro.as_ref(), &melange);
    let sortie = noyau::systeme::commande(&cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).output();
    match sortie {
        Ok(o) if o.status.success() => match std::fs::rename(&melange, &cible) {
            Ok(()) => {
                nettoyer();
                Fin::Prise(cible, video.premiere.duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()))
            }
            Err(e) => Fin::Erreur(format!("Prise gardée sous {} : {e}", melange.display())),
        },
        resultat => {
            let _ = std::fs::remove_file(&melange);
            let raison = match resultat {
                Ok(o) => String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("ffmpeg a échoué").to_string(),
                Err(e) => e.to_string(),
            };
            // L'image d'abord : la prise existe, sans son ; les WAV restent à côté pour qu'on les retrouve.
            match std::fs::rename(&video.chemin, &cible) {
                Ok(()) => Fin::Erreur(format!("Prise enregistrée sans son ({raison}) : {}", cible.display())),
                Err(e) => Fin::Erreur(format!("Vidéo gardée sous {} : {e}", video.chemin.display())),
            }
        }
    }
}

/// Les sons voulus pour une prise.
#[derive(Clone, Copy)]
pub struct Sons {
    pub bureau: bool,
    pub micro: bool,
}

struct EnCours {
    source: Box<dyn Source>,
    arret: Arc<AtomicBool>,
    fil: JoinHandle<Result<Option<Video>, String>>,
    sons: Vec<Option<media::sons::PisteSon>>,
}

/// Une prise du Monteur, du décompte à la prise rangée. Mêmes questions que `capture::Capture` sous Linux.
pub struct Enregistrement {
    dossier: PathBuf,
    demarrer: Option<Demarrer>,
    sons_voulus: Sons,
    decompte: Option<Instant>,
    en_cours: Option<EnCours>,
    debut: Arc<Mutex<Option<Instant>>>,
    micro: Option<media::sons::Micro>,
    travail_fichiers: Vec<PathBuf>,
    /// Les secondes filmées, figées à l'arrêt.
    pub duree: Option<f64>,
    arret: Option<Instant>,
    annulee: bool,
    fini: bool,
    travail: Option<(Receiver<Fin>, JoinHandle<()>)>,
}

impl Enregistrement {
    /// `avec_decompte` : trois bips, puis la première image.
    pub fn lancer(dossier: &Path, demarrer: Demarrer, sons: Sons, avec_decompte: bool) -> Result<Self, String> {
        std::fs::create_dir_all(dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
        let mut e = Self {
            dossier: dossier.to_path_buf(),
            demarrer: Some(demarrer),
            sons_voulus: sons,
            decompte: None,
            en_cours: None,
            debut: Arc::new(Mutex::new(None)),
            micro: media::sons::Micro::lancer(SEUIL_MUET),
            travail_fichiers: Vec::new(),
            duree: None,
            arret: None,
            annulee: false,
            fini: false,
            travail: None,
        };
        if avec_decompte {
            e.decompte = Some(Instant::now());
            // Les contrôles ne sonnent pas.
            if !cfg!(test) {
                std::thread::spawn(|| media::sons::jouer_des_bips(&[660.0, 660.0, 990.0]));
            }
        } else {
            e.commencer()?;
        }
        Ok(e)
    }

    fn commencer(&mut self) -> Result<(), String> {
        let Some(demarrer) = self.demarrer.take() else { return Ok(()) };
        let base = format!("capture {}", noyau::maintenant().replace([':', '/'], "-"));
        let video = self.dossier.join(format!("{base}.partiel.mp4"));
        let (wav_bureau, wav_micro) = (self.dossier.join(format!("{base} bureau.partiel.wav")), self.dossier.join(format!("{base} micro.partiel.wav")));
        self.travail_fichiers = vec![video.clone(), wav_bureau.clone(), wav_micro.clone()];
        let dernier: Dernier = Arc::new(Mutex::new(Boite::default()));
        let source = demarrer(dernier.clone())?;
        let sons = vec![
            self.sons_voulus.bureau.then(|| media::sons::PisteSon::bureau(&wav_bureau)).flatten(),
            self.sons_voulus.micro.then(|| media::sons::PisteSon::micro(&wav_micro)).flatten(),
        ];
        let arret = Arc::new(AtomicBool::new(false));
        let (a, d) = (arret.clone(), self.debut.clone());
        let fil = std::thread::Builder::new()
            .name("filmer".into())
            .spawn(move || filmer(dernier, a, video, d))
            .map_err(|e| e.to_string())?;
        self.en_cours = Some(EnCours { source, arret, fil, sons });
        Ok(())
    }

    /// Les secondes filmées ; `None` avant la première image.
    pub fn temps(&self) -> Option<f64> {
        self.debut.lock().ok().and_then(|d| d.map(|t| t.elapsed().as_secs_f64()))
    }

    pub fn niveau(&self) -> Option<f32> {
        self.micro.as_ref().map(media::sons::Micro::niveau)
    }

    pub fn micro_muet(&self) -> bool {
        self.micro.as_ref().is_some_and(|m| m.muet_depuis() > Duration::from_secs(5))
    }

    pub fn decompte_restant(&self) -> Option<u32> {
        self.decompte.filter(|_| self.en_cours.is_none()).map(|d| (DECOMPTE - d.elapsed().as_secs_f64()).ceil().max(1.0) as u32)
    }

    pub fn en_finition(&self) -> bool {
        self.travail.is_some()
    }

    pub fn en_arret(&self) -> bool {
        self.arret.is_some() && !self.fini
    }

    /// Pendant le décompte : rien n'est filmé, c'est annulé. Ensuite : la source s'arrête, la vidéo se termine.
    pub fn arreter(&mut self) {
        if self.arret.is_some() {
            return;
        }
        self.arret = Some(Instant::now());
        match &mut self.en_cours {
            None => self.annulee = true,
            Some(c) => {
                c.arret.store(true, Ordering::Relaxed);
                c.source.arreter();
            }
        }
    }

    /// À appeler à chaque tour ; `Some` une seule fois, quand la prise est rangée (ou ratée).
    pub fn suivre(&mut self) -> Option<Fin> {
        if self.fini {
            return None;
        }
        if let Some((recu, _)) = &self.travail {
            let fin = recu.try_recv().ok()?;
            self.fini = true;
            return Some(fin);
        }
        if self.annulee {
            self.fini = true;
            self.micro = None;
            return Some(Fin::Annulee);
        }
        if let Some(d) = self.decompte
            && self.en_cours.is_none()
            && d.elapsed().as_secs_f64() >= DECOMPTE
            && let Err(e) = self.commencer()
        {
            self.fini = true;
            return Some(Fin::Erreur(format!("Enregistrement impossible : {e}")));
        }
        // Le fil s'arrête seul si la fenêtre se ferme ou que ffmpeg lâche.
        if !self.en_cours.as_ref().is_some_and(|c| c.fil.is_finished()) {
            return None;
        }
        let (fin_video, sons) = self.terminer();
        let (dossier, t0, travail) = (self.dossier.clone(), self.debut.lock().ok().and_then(|d| *d), self.travail_fichiers.clone());
        let (envoi, recu) = mpsc::channel();
        let fil = std::thread::spawn(move || {
            let _ = envoi.send(conclure(&dossier, fin_video, sons, t0, &travail));
        });
        self.travail = Some((recu, fil));
        None
    }

    /// Arrête la source et les sons, attend la vidéo.
    fn terminer(&mut self) -> (Result<Option<Video>, String>, Vec<Result<media::sons::PisteFinie, String>>) {
        let Some(mut c) = self.en_cours.take() else { return (Ok(None), Vec::new()) };
        c.arret.store(true, Ordering::Relaxed);
        c.source.arreter();
        let video = c.fil.join().unwrap_or_else(|_| Err("le fil qui filme s'est arrêté".into()));
        let sons = c.sons.into_iter().map(|s| s.ok_or_else(|| "pas de son".to_string()).and_then(media::sons::PisteSon::finir)).collect();
        self.duree = self.temps();
        if let Ok(Some(v)) = &video {
            self.duree = Some(v.nombre as f64 / IMAGES_PAR_S as f64);
        }
        self.micro = None;
        (video, sons)
    }
}

/// Le Monteur se ferme en filmant : la prise se termine quand même.
impl Drop for Enregistrement {
    fn drop(&mut self) {
        if let Some((_, fil)) = self.travail.take() {
            let _ = fil.join();
            return;
        }
        if self.en_cours.is_none() {
            return;
        }
        let (video, sons) = self.terminer();
        let t0 = self.debut.lock().ok().and_then(|d| *d);
        conclure(&self.dossier, video, sons, t0, &self.travail_fichiers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(l: u32, h: u32, v: u8) -> Image {
        Image { largeur: l, hauteur: h, bgra: vec![v; (l * h * 4) as usize] }
    }

    #[test]
    fn une_image_se_pose_en_haut_a_gauche() {
        let mut cadre = vec![9u8; 4 * 3 * 4];
        // Plus petite : le reste est noir.
        poser(&image(2, 2, 7), &mut cadre, 4, 3);
        assert_eq!(&cadre[..8], &[7; 8]);
        assert_eq!(&cadre[8..16], &[0; 8]);
        assert!(cadre[32..].iter().all(|&o| o == 0), "la dernière ligne est noire");
        // Plus grande : coupée.
        poser(&image(6, 5, 5), &mut cadre, 4, 3);
        assert!(cadre.iter().all(|&o| o == 5));
    }

    #[test]
    fn la_zone_cliente_dans_l_image() {
        // Fenêtre à (100, 50), zone cliente 31 px plus bas (la barre de titre), 1920 × 1129 de 1920 × 1160.
        assert_eq!(zone_cliente((100, 50), (100, 81), (1920, 1129), (1920, 1160)), Some((0, 31, 1920, 1160)));
        // Agrandie : le cadre visible commence à 0, la zone cliente aussi, sous la barre.
        assert_eq!(zone_cliente((0, 0), (0, 23), (2560, 1377), (2560, 1400)), Some((0, 23, 2560, 1400)));
        // Des mesures qui ne tiennent pas : l'image entière.
        assert_eq!(zone_cliente((100, 50), (90, 50), (800, 600), (800, 600)), None);
        assert_eq!(zone_cliente((0, 0), (0, 0), (0, 0), (800, 600)), None);
    }

    #[test]
    fn le_melange_cale_chaque_son_sur_la_premiere_image() {
        let b = SonAMeler { chemin: "b.wav".into(), decalage: -0.250, canaux: 2 };
        let m = SonAMeler { chemin: "m.wav".into(), decalage: 0.1, canaux: 1 };
        let c = commande_melange(Path::new("v.mp4"), 12.5, Some(&b), Some(&m), Path::new("s.mp4")).join(" ");
        assert!(c.contains("-ss 0.250 -i b.wav"), "{c}");
        assert!(c.contains("-itsoffset 0.100 -i m.wav"), "{c}");
        assert!(c.contains("[2:a]pan=stereo|c0=c0|c1=c0[voix];[1:a][voix]amix"), "un micro mono va des deux côtés : {c}");
        assert!(c.contains("-c:v copy -t 12.500 s.mp4"), "{c}");
        let seul = commande_melange(Path::new("v.mp4"), 1.0, None, Some(&SonAMeler { canaux: 2, ..m }), Path::new("s.mp4")).join(" ");
        assert!(seul.contains("[1:a]pan=stereo|c0=c0+c1|c1=c0+c1[son]"), "{seul}");
        let muet = commande_melange(Path::new("v.mp4"), 1.0, None, None, Path::new("s.mp4")).join(" ");
        assert!(!muet.contains("filter_complex") && muet.contains("-map 0:v"), "{muet}");
    }

    /// Une source factice : une image qui change de couleur toutes les 100 ms, dans un fil.
    struct Factice {
        arret: Arc<AtomicBool>,
    }

    impl Source for Factice {
        fn arreter(&mut self) {
            self.arret.store(true, Ordering::Relaxed);
        }
    }

    fn factice() -> Demarrer {
        Box::new(|dernier: Dernier| {
            let arret = Arc::new(AtomicBool::new(false));
            let a = arret.clone();
            std::thread::spawn(move || {
                let mut n = 0u8;
                while !a.load(Ordering::Relaxed) {
                    // Une taille impaire : le cadre la ramène à 320 × 240.
                    let i = Arc::new(image(321, 241, n.wrapping_mul(40)));
                    if let Ok(mut b) = dernier.lock() {
                        b.image = Some(i);
                    }
                    n = n.wrapping_add(1);
                    std::thread::sleep(Duration::from_millis(100));
                }
            });
            Ok(Box::new(Factice { arret }) as Box<dyn Source>)
        })
    }

    fn sonder(f: &Path) -> String {
        let o = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration:stream=codec_type,width,height,nb_read_frames", "-count_frames", "-of", "csv=p=0"])
            .arg(f)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    /// Deux secondes filmées sans son : une prise au nom d'OBS, 30 i/s, à la taille paire de la première image, et
    /// rien qui traîne.
    #[test]
    fn une_prise_de_deux_secondes() {
        let d = std::env::temp_dir().join(format!("monteur-enregistreur-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut e = Enregistrement::lancer(&d, factice(), Sons { bureau: false, micro: false }, false).unwrap();
        let limite = Instant::now() + Duration::from_secs(30);
        let mut fin = None;
        while fin.is_none() && Instant::now() < limite {
            if e.temps().is_some_and(|t| t >= 2.0) && !e.en_arret() {
                e.arreter();
            }
            fin = e.suivre();
            std::thread::sleep(Duration::from_millis(20));
        }
        let Some(Fin::Prise(p, _)) = fin else { panic!("pas de prise") };
        let infos = sonder(&p);
        let restes: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(Result::ok).map(|f| f.file_name().to_string_lossy().into_owned()).collect();
        let _ = std::fs::remove_dir_all(&d);
        assert!(infos.contains("video,320,240"), "{infos}");
        let images: u64 = infos.lines().find(|l| l.starts_with("video")).and_then(|l| l.rsplit(',').next()).and_then(|n| n.trim().parse().ok()).unwrap_or(0);
        assert!((60..=63).contains(&images), "{images} images pour 2 s");
        assert_eq!(restes.len(), 1, "seule la prise reste : {restes:?}");
        let nom = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nom.len() == "2026-11-22 22-00-00.mp4".len() && nom.ends_with(".mp4") && nom.as_bytes()[10] == b' ', "un nom à la façon d'OBS : {nom}");
    }

    /// Arrêté pendant le décompte : rien n'est filmé.
    #[test]
    fn annule_pendant_le_decompte() {
        let d = std::env::temp_dir().join(format!("monteur-decompte-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut e = Enregistrement::lancer(&d, factice(), Sons { bureau: false, micro: false }, true).unwrap();
        assert_eq!(e.decompte_restant(), Some(3));
        e.arreter();
        assert!(matches!(e.suivre(), Some(Fin::Annulee)));
        let vide = std::fs::read_dir(&d).map(|r| r.count() == 0).unwrap_or(true);
        let _ = std::fs::remove_dir_all(&d);
        assert!(vide);
    }

    /// Le mélange pour de bon : une vidéo de 2 s, un son « bureau » commencé 0,5 s avant elle, un micro mono commencé
    /// 0,25 s après ; la prise a une seule piste son, stéréo, et dure 2 s.
    #[test]
    fn le_melange_pour_de_bon() {
        let d = std::env::temp_dir().join(format!("monteur-melange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (v, b, m, s) = (d.join("v.mp4"), d.join("b.wav"), d.join("m.wav"), d.join("s.mp4"));
        let ok = |args: &[&str]| std::process::Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success();
        assert!(ok(&["-f", "lavfi", "-i", "color=red:s=64x48:r=30:d=2", "-c:v", "libx264", "-pix_fmt", "yuv420p", v.to_str().unwrap()]));
        assert!(ok(&["-f", "lavfi", "-i", "sine=f=440:d=3", "-ac", "2", b.to_str().unwrap()]));
        assert!(ok(&["-f", "lavfi", "-i", "sine=f=660:d=2", "-ac", "1", m.to_str().unwrap()]));
        let bureau = SonAMeler { chemin: b, decalage: -0.5, canaux: 2 };
        let micro = SonAMeler { chemin: m, decalage: 0.25, canaux: 1 };
        let cmd = commande_melange(&v, 2.0, Some(&bureau), Some(&micro), &s);
        assert!(std::process::Command::new(&cmd[0]).args(&cmd[1..]).status().unwrap().success());
        let o = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration:stream=codec_type,channels", "-of", "csv=p=0"])
            .arg(&s)
            .output()
            .unwrap();
        let infos = String::from_utf8_lossy(&o.stdout).into_owned();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(infos.matches("audio").count(), 1, "{infos}");
        assert!(infos.contains("audio,2"), "stéréo : {infos}");
        let duree: f64 = infos.lines().last().unwrap().trim().parse().unwrap();
        assert!((duree - 2.0).abs() < 0.1, "{duree}");
    }
}
