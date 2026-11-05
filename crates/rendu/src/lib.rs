//! Rendre le montage d'une prise : un ffmpeg décode la vidéo en images brutes, chacune est peinte
//! par le composeur (le dessin de la visionneuse : ce qu'on voit est ce qui sort), sur plusieurs
//! cœurs, et un second ffmpeg les encode avec le son, coupes retirées. Mêmes réglages que le Monteur 1
//! (x264, crf 18, AAC 192k). La vidéo n'est jamais touchée ; le rendu s'écrit en `.partiel.mp4`,
//! renommé une fois fini.
//!
//! Les vidéos insérées : une **intercalée** passe entre deux images de la prise, avec son son ; une
//! vidéo **par-dessus** remplace l'image, la voix et ses sous-titres continuent.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

use composeur::dessin::{Compositeur, R};
use composeur::police::Polices;
use noyau::Erreur;
use noyau::montage::{Insertion, Montage};
use noyau::prereglages::Prereglage;
use noyau::sous_titres::{image_gardee, son_retire};
use noyau::videos::{self, Infos};
use noyau::voix::Voix;
use tiny_skia::{IntSize, Pixmap};

/// Où en est un rendu, lisible depuis un autre fil.
#[derive(Default)]
pub struct Suivi {
    /// Images écrites, et combien il y en aura.
    pub faites: AtomicU32,
    pub total: AtomicU32,
    /// Mis à vrai pour arrêter.
    pub arret: AtomicBool,
    /// Après le rendu : on colle la vignette.
    pub vignette: AtomicBool,
    /// La durée d'un court ou d'un GIF est atteinte : la lecture s'arrête, le rendu n'est pas perdu.
    pub assez: AtomicBool,
}

impl Suivi {
    fn fin_de_lecture(&self) -> bool {
        self.arret.load(Ordering::Relaxed) || self.assez.load(Ordering::Relaxed)
    }

    pub fn fraction(&self) -> f64 {
        let t = self.total.load(Ordering::Relaxed).max(1);
        (self.faites.load(Ordering::Relaxed) as f64 / t as f64).min(1.0)
    }
}

/// La cadence, en fraction exacte : « 30/1 », « 30000/1001 ».
pub fn cadence(texte: &str) -> (u32, u32) {
    let mut p = texte.split('/');
    let a: u32 = p.next().and_then(|x| x.trim().parse().ok()).unwrap_or(30);
    let b: u32 = p.next().and_then(|x| x.trim().parse().ok()).unwrap_or(1);
    if a == 0 || b == 0 || a as f64 / b as f64 > 120.0 { (30, 1) } else { (a, b) }
}

fn s(x: &str) -> String {
    x.to_string()
}

pub fn commande_decodage(video: &Path, ips: &str) -> Vec<String> {
    vec![
        s("ffmpeg"), s("-v"), s("error"), s("-i"), video.to_string_lossy().into_owned(), s("-an"), s("-vf"),
        format!("fps={ips}"), s("-f"), s("rawvideo"), s("-pix_fmt"), s("rgba"), s("-"),
    ]
}

/// Une vidéo insérée, à la cadence et à la taille de la sortie, sans la déformer.
pub fn commande_clip(chemin: &Path, debut: f64, duree: f64, l: u32, h: u32, ips: &str) -> Vec<String> {
    vec![
        s("ffmpeg"), s("-v"), s("error"), s("-ss"), format!("{debut:.3}"), s("-i"), chemin.to_string_lossy().into_owned(),
        s("-t"), format!("{duree:.3}"), s("-an"), s("-vf"),
        format!("fps={ips},scale={l}:{h}:force_original_aspect_ratio=decrease,pad={l}:{h}:(ow-iw)/2:(oh-ih)/2:color=black"),
        s("-f"), s("rawvideo"), s("-pix_fmt"), s("rgba"), s("-"),
    ]
}

/// L'expression ffmpeg vraie dans les passages à retirer.
pub fn expression_coupes(coupes: &[(f64, f64)]) -> Option<String> {
    (!coupes.is_empty()).then(|| coupes.iter().map(|(a, b)| format!("between(t,{a:.4},{b:.4})")).collect::<Vec<_>>().join("+"))
}

/// Une vidéo intercalée : son fichier et a-t-elle du son ?
pub struct ClipSon<'a> {
    pub insertion: &'a Insertion,
    pub chemin: PathBuf,
    pub son: bool,
}

/// Le graphe du son. Sans intercalée, celui du Monteur 1. Avec : la prise est découpée aux instants
/// des intercalées, et leur son (ou un silence) se glisse entre les morceaux.
fn graphe_son(retire: &[(f64, f64)], clips: &[ClipSon]) -> String {
    let expr = expression_coupes(retire);
    let selection = expr.as_ref().map(|e| format!("aselect='not({e})',asetpts=N/SR/TB")).unwrap_or_else(|| s("asetpts=N/SR/TB"));
    if clips.is_empty() {
        let f = expr.map(|e| format!("aselect='not({e})',asetpts=N/SR/TB")).unwrap_or_else(|| s("anull"));
        return format!("[1:a]{f}[a]");
    }
    const FORMAT: &str = "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo";
    let n = clips.len() + 1;
    let mut g: Vec<String> = Vec::new();
    let sorties: String = (0..n).map(|i| format!("[p{i}]")).collect();
    g.push(format!("[1:a]asplit={n}{sorties}"));
    let mut concat = String::new();
    let mut debut = 0.0;
    let mut entree_clip = 2; // 0 : les images, 1 : la prise, puis les intercalées qui ont du son
    for (i, c) in clips.iter().enumerate() {
        let fin = c.insertion.t;
        g.push(format!("[p{i}]atrim=start={debut:.4}:end={fin:.4},{selection},{FORMAT}[s{i}]"));
        let d = c.insertion.duree();
        if c.son {
            g.push(format!(
                "[{entree_clip}:a]atrim=start={:.4}:end={:.4},asetpts=N/SR/TB,{FORMAT},apad=whole_dur={d:.4}[c{i}]",
                c.insertion.debut, c.insertion.fin
            ));
            entree_clip += 1;
        } else {
            g.push(format!("anullsrc=r=48000:cl=stereo,atrim=duration={d:.4},aformat=sample_fmts=fltp:channel_layouts=stereo[c{i}]"));
        }
        concat.push_str(&format!("[s{i}][c{i}]"));
        debut = fin;
    }
    let dernier = n - 1;
    g.push(format!("[p{dernier}]atrim=start={debut:.4},{selection},{FORMAT}[s{dernier}]"));
    concat.push_str(&format!("[s{dernier}]"));
    g.push(format!("{concat}concat=n={}:v=0:a=1[a]", clips.len() * 2 + 1));
    g.join(";")
}

#[allow(clippy::too_many_arguments)]
pub fn commande_encodage(video: &Path, sortie: &Path, l: u32, h: u32, ips: &str, retire: &[(f64, f64)], son: bool, clips: &[ClipSon], volume: Option<&str>, pre: &Prereglage) -> Vec<String> {
    let mut cmd = vec![
        s("ffmpeg"), s("-v"), s("error"), s("-y"), s("-nostats"), s("-f"), s("rawvideo"), s("-pix_fmt"), s("rgba"),
        s("-s"), format!("{l}x{h}"), s("-r"), s(ips), s("-i"), s("-"),
    ];
    if son {
        cmd.extend([s("-i"), video.to_string_lossy().into_owned()]);
        for c in clips.iter().filter(|c| c.son) {
            cmd.extend([s("-i"), c.chemin.to_string_lossy().into_owned()]);
        }
        let (graphe, sortie_son) = match volume {
            Some(v) => (format!("{};[a]{v}[fort]", graphe_son(retire, clips)), "[fort]"),
            None => (graphe_son(retire, clips), "[a]"),
        };
        cmd.extend([s("-filter_complex"), graphe, s("-map"), s("0:v"), s("-map"), s(sortie_son)]);
        cmd.extend([s("-c:a"), s("aac"), s("-b:a"), s("192k"), s("-shortest")]);
    } else {
        cmd.extend([s("-map"), s("0:v")]);
    }
    if let Some(f) = pre.image {
        cmd.extend([s("-vf"), s(f)]);
    }
    if let Some((crf, vitesse)) = pre.x264 {
        cmd.extend(["-c:v", "libx264", "-crf", crf, "-preset", vitesse, "-pix_fmt", "yuv420p", "-movflags", "+faststart"].map(String::from));
    }
    cmd.push(sortie.to_string_lossy().into_owned());
    cmd
}

/// La chaîne de la voix pour tout le son monté : nettoyage, puis niveau mesuré (premier passage de
/// `loudnorm`, repris au second). `None` : rien à appliquer.
fn filtre_son(video: &Path, retire: &[(f64, f64)], clips: &[ClipSon], voix: &Voix) -> Option<String> {
    let nettoyage = voix.filtres();
    if !voix.niveau {
        return nettoyage;
    }
    let mut cmd = vec![s("-hide_banner"), s("-nostats"), s("-f"), s("lavfi"), s("-i"), s("nullsrc=s=2x2:d=0.04"), s("-i"), video.to_string_lossy().into_owned()];
    for c in clips.iter().filter(|c| c.son) {
        cmd.extend([s("-i"), c.chemin.to_string_lossy().into_owned()]);
    }
    let avant = nettoyage.as_ref().map(|f| format!("{f},")).unwrap_or_default();
    let graphe = format!("{};[a]{avant}loudnorm={}:print_format=json[mesure]", graphe_son(retire, clips), voix.cible());
    cmd.extend([s("-filter_complex"), graphe, s("-map"), s("[mesure]"), s("-f"), s("null"), s("-")]);
    let sortie = Command::new("ffmpeg").args(&cmd).stdin(Stdio::null()).output().ok();
    let niveau = sortie.and_then(|o| filtre_depuis_mesure(&String::from_utf8_lossy(&o.stderr), voix));
    match (avant.is_empty(), niveau) {
        (_, None) => nettoyage,
        (true, Some(n)) => Some(n),
        (false, Some(n)) => Some(format!("{avant}{n}")),
    }
}

/// Le bloc JSON que `loudnorm` écrit en fin de sortie d'erreur.
fn lire_mesure(journal: &str) -> Option<[f64; 5]> {
    let bloc = &journal[journal.rfind('{')?..=journal.rfind('}')?];
    let m: serde_json::Value = serde_json::from_str(bloc).ok()?;
    let champ = |n: &str| m.get(n)?.as_str()?.parse::<f64>().ok().filter(|v| v.is_finite());
    Some([champ("input_i")?, champ("input_tp")?, champ("input_lra")?, champ("input_thresh")?, champ("target_offset")?])
}

/// Le gain mesuré, puis un limiteur pour les crêtes. Pas de second passage de `loudnorm` : sur la
/// voix, ses crêtes le font basculer en mode dynamique et il rate la cible de 2 à 3 LU. Mesuré sur
/// une prise OBS : −14,2 LUFS, crête −0,7 dB. `None` : pas de mesure, ou un silence.
fn filtre_depuis_mesure(journal: &str, voix: &Voix) -> Option<String> {
    let [i, ..] = lire_mesure(journal)?;
    if i < -60.0 {
        return None;
    }
    // Marge de 0,5 dB : le limiteur voit les échantillons, pas les crêtes entre eux.
    let limite = 10f64.powf((voix.crete_db - 0.5) / 20.0).clamp(0.0625, 1.0);
    Some(format!("volume={:.2}dB,alimiter=limit={limite:.4}:attack=5:release=50:level=false", voix.niveau_lufs - i))
}

/// Un extrait pour l'écoute : son stéréo 48 kHz en s16le, et son niveau en LUFS.
pub struct Extrait {
    pub pcm: Vec<u8>,
    pub lufs: Option<f64>,
}

fn ffmpeg_extrait(video: &Path, debut: f64, duree: f64, filtre: Option<&str>, sortie: &[&str]) -> Option<std::process::Output> {
    let mut c = Command::new("ffmpeg");
    c.args(["-hide_banner", "-nostats", "-ss", &format!("{debut:.3}"), "-t", &format!("{duree:.3}"), "-i"]).arg(video).arg("-vn");
    if let Some(f) = filtre {
        c.args(["-af", f]);
    }
    c.args(sortie).stdin(Stdio::null()).output().ok()
}

/// Le passage tel quel, puis nettoyé comme au rendu.
pub fn avant_apres(video: &Path, debut: f64, duree: f64, voix: &Voix) -> Option<(Extrait, Extrait)> {
    const PCM: [&str; 9] = ["-ac", "2", "-ar", "48000", "-f", "s16le", "-c:a", "pcm_s16le", "-"];
    const NUL: [&str; 3] = ["-f", "null", "-"];
    let mesure = |f: &str| ffmpeg_extrait(video, debut, duree, Some(f), &NUL).map(|o| String::from_utf8_lossy(&o.stderr).into_owned());
    let vers_cible = format!("loudnorm={}:print_format=json", voix.cible());
    let brut = mesure(&vers_cible)?;
    let nettoyage = voix.filtres();
    let avant_niveau = nettoyage.as_ref().map(|f| format!("{f},")).unwrap_or_default();
    let filtre = if voix.niveau {
        let m = mesure(&format!("{avant_niveau}{vers_cible}"))?;
        filtre_depuis_mesure(&m, voix).map(|n| format!("{avant_niveau}{n}")).or(nettoyage)
    } else {
        nettoyage
    };
    let lire = |f: Option<&str>| ffmpeg_extrait(video, debut, duree, f, &PCM).filter(|o| o.status.success()).map(|o| o.stdout);
    let (pcm_avant, pcm_apres) = (lire(None)?, lire(filtre.as_deref())?);
    // Mesurée sur ce qui sort vraiment, pas sur la cible.
    let lufs = |f: Option<&str>| {
        let texte = match f {
            Some(f) => format!("{f},{}", vers_cible),
            None => vers_cible.clone(),
        };
        mesure(&texte).and_then(|m| lire_mesure(&m)).map(|m| m[0])
    };
    Some((Extrait { pcm: pcm_avant, lufs: lire_mesure(&brut).map(|m| m[0]) }, Extrait { pcm: pcm_apres, lufs: lufs(filtre.as_deref()) }))
}

fn lancer(cmd: &[String], entree: bool) -> Result<Child, Erreur> {
    Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(if entree { Stdio::piped() } else { Stdio::null() })
        .stdout(if entree { Stdio::null() } else { Stdio::piped() })
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Erreur(format!("{} : {e}", cmd[0])))
}

fn lire_image(flux: &mut ChildStdout, n: usize) -> Option<Vec<u8>> {
    let mut b = vec![0u8; n];
    flux.read_exact(&mut b).ok()?;
    Some(b)
}

enum Image {
    /// Une image de la prise, à peindre à l'instant t.
    Prise { t: f64, rgba: Vec<u8> },
    /// Par-dessus : déjà à la taille de sortie ; seuls les sous-titres s'y posent.
    Dessus { t: f64, rgba: Vec<u8> },
    /// Intercalée : elle sort telle quelle.
    Clip { rgba: Vec<u8> },
}

/// Ce que le fil de lecture emporte.
struct Lecture {
    video: PathBuf,
    dossier: PathBuf,
    montage: Arc<Montage>,
    ips_texte: String,
    ips: f64,
    l: u32,
    h: u32,
    lo: u32,
    ho: u32,
}

impl Lecture {
    fn chemin(&self, i: &Insertion) -> PathBuf {
        self.dossier.join(&i.video)
    }

    /// Lit la prise et les vidéos insérées, dans l'ordre de la sortie.
    fn derouler(&self, envoi: SyncSender<(u64, Image)>, suivi: &Suivi) -> Result<(), Erreur> {
        let (l, h, lo, ho) = (self.l as usize, self.h as usize, self.lo as usize, self.ho as usize);
        let mut decodeur = lancer(&commande_decodage(&self.video, &self.ips_texte), false)?;
        let mut flux = decodeur.stdout.take().expect("sortie du décodeur");
        let intercalees: Vec<&Insertion> = self.montage.insertions.iter().filter(|i| !i.par_dessus()).collect();
        let mut prochaine = 0;
        let mut ordre = 0u64;
        let mut dessus: Option<(String, f64, Child, ChildStdout)> = None;
        let mut n = 0u64;
        let mut lues = 0u64;
        while !suivi.fin_de_lecture() {
            let Some(rgba) = lire_image(&mut flux, l * h * 4) else { break };
            let t = n as f64 / self.ips;
            n += 1;
            lues += 1;
            while prochaine < intercalees.len() && intercalees[prochaine].t <= t {
                self.intercaler(intercalees[prochaine], &envoi, &mut ordre, suivi)?;
                prochaine += 1;
            }
            // Par-dessus : une image de la vidéo pour chaque image de la prise, gardée ou non,
            // pour que les deux restent ensemble à travers une coupe.
            let mut remplace = None;
            match self.montage.dessus_a(t) {
                Some((i, _)) => {
                    let cle = (i.video.clone(), i.t);
                    if dessus.as_ref().is_none_or(|d| d.0 != cle.0 || d.1 != cle.1) {
                        if let Some(mut d) = dessus.take() {
                            let _ = d.2.kill();
                            let _ = d.2.wait();
                        }
                        let mut c = lancer(&commande_clip(&self.chemin(i), i.debut, i.duree(), self.lo, self.ho, &self.ips_texte), false)?;
                        let f = c.stdout.take().expect("sortie du clip");
                        dessus = Some((cle.0, cle.1, c, f));
                    }
                    remplace = dessus.as_mut().and_then(|d| lire_image(&mut d.3, lo * ho * 4));
                }
                None => {
                    if let Some(mut d) = dessus.take() {
                        let _ = d.2.kill();
                        let _ = d.2.wait();
                    }
                }
            }
            if !image_gardee(&self.montage, t, self.ips) {
                continue;
            }
            let image = match remplace {
                Some(rgba) => Image::Dessus { t, rgba },
                None => Image::Prise { t, rgba },
            };
            if envoi.send((ordre, image)).is_err() {
                break;
            }
            ordre += 1;
        }
        // Une intercalée posée à la toute fin.
        while prochaine < intercalees.len() && !suivi.fin_de_lecture() {
            self.intercaler(intercalees[prochaine], &envoi, &mut ordre, suivi)?;
            prochaine += 1;
        }
        drop(flux);
        let _ = decodeur.kill();
        let _ = decodeur.wait();
        if let Some(mut d) = dessus {
            let _ = d.2.kill();
            let _ = d.2.wait();
        }
        if lues == 0 {
            return Err(Erreur("aucune image lue dans la vidéo".into()));
        }
        Ok(())
    }

    /// Une vidéo intercalée : exactement sa durée, en images. Plus courte que prévu, la dernière
    /// image reste : le son et l'image ne se décalent jamais.
    fn intercaler(&self, i: &Insertion, envoi: &SyncSender<(u64, Image)>, ordre: &mut u64, suivi: &Suivi) -> Result<(), Erreur> {
        let n = (i.duree() * self.ips).round() as u64;
        let taille = self.lo as usize * self.ho as usize * 4;
        let mut c = lancer(&commande_clip(&self.chemin(i), i.debut, i.duree(), self.lo, self.ho, &self.ips_texte), false)?;
        let mut f = c.stdout.take().expect("sortie du clip");
        let mut derniere = vec![0u8; taille];
        for k in 0..taille / 4 {
            derniere[k * 4 + 3] = 255;
        }
        for _ in 0..n {
            if suivi.fin_de_lecture() {
                break;
            }
            if let Some(image) = lire_image(&mut f, taille) {
                derniere = image;
            }
            if envoi.send((*ordre, Image::Clip { rgba: derniere.clone() })).is_err() {
                break;
            }
            *ordre += 1;
        }
        drop(f);
        let _ = c.kill();
        let _ = c.wait();
        Ok(())
    }
}

/// Peint une image ; un fil par cœur en fait tourner une à la fois.
fn peindre(image: Image, c: &Compositeur, voix: &Compositeur, l: u32, h: u32, lo: u32, ho: u32) -> Option<Vec<u8>> {
    let cible = R::new(0.0, 0.0, lo as f64, ho as f64);
    match image {
        Image::Clip { rgba } => Some(rgba),
        Image::Dessus { t, rgba } => {
            let mut p = Pixmap::from_vec(rgba, IntSize::from_wh(lo, ho)?)?;
            voix.peindre_calque(&mut p.as_mut(), t, cible, (0.0, 0.0, lo as f64, ho as f64), lo as f64, ho as f64);
            Some(p.take())
        }
        Image::Prise { t, rgba } => {
            let source = Pixmap::from_vec(rgba, IntSize::from_wh(l, h)?)?;
            let mut p = Pixmap::new(lo, ho)?;
            c.peindre(&mut p.as_mut(), Some(source.as_ref()), t, cible, false);
            Some(p.take())
        }
    }
}

/// Rend le montage dans `sortie` (écrit en `.partiel.mp4`, renommé une fois fini).
pub fn rendre(video: &Path, montage: &Montage, voix: &Voix, pre: &Prereglage, sortie: &Path, suivi: &Suivi) -> Result<(), Erreur> {
    let infos = sonder(video)?;
    let (num, den) = cadence(&infos.images_par_s);
    let ips_texte = format!("{num}/{den}");
    let ips = num as f64 / den as f64;
    let (l, h) = (infos.largeur as u32, infos.hauteur as u32);
    let (lo, ho) = (l - l % 2, h - h % 2);
    let dossier = video.parent().unwrap_or(Path::new(".")).to_path_buf();
    let partiel = videos::chemin_partiel(sortie);
    let retire = son_retire(montage);

    let clips: Vec<ClipSon> = montage
        .insertions
        .iter()
        .filter(|i| !i.par_dessus())
        .map(|insertion| {
            let chemin = dossier.join(&insertion.video);
            let son = sonder(&chemin).is_ok_and(|i| i.son);
            ClipSon { insertion, chemin, son }
        })
        .collect();
    for i in &montage.insertions {
        if !dossier.join(&i.video).exists() {
            return Err(Erreur(format!("Vidéo insérée introuvable : {}", i.video)));
        }
    }
    let son = infos.son && pre.son;
    let volume = if son { filtre_son(video, &retire, &clips, voix) } else { None };
    let mut encodeur = lancer(&commande_encodage(video, &partiel, lo, ho, &ips_texte, &retire, son, &clips, volume.as_deref(), pre), true)?;
    let mut entree = encodeur.stdin.take().expect("entrée de l'encodeur");
    let erreurs = encodeur.stderr.take();
    // Lue à part : ffmpeg bloquerait s'il remplissait sa sortie d'erreur.
    let erreurs = std::thread::spawn(move || {
        let mut t = String::new();
        if let Some(mut e) = erreurs {
            let _ = e.read_to_string(&mut t);
        }
        t
    });

    let total = pre.duree_max.map_or(montage.duree_montee(), |d| d.min(montage.duree_montee()));
    let limite = pre.duree_max.map(|d| (d * ips) as u64);
    suivi.total.store((total * ips) as u32, Ordering::Relaxed);
    suivi.assez.store(false, Ordering::Relaxed);
    suivi.faites.store(0, Ordering::Relaxed);
    let m = Arc::new(montage.clone());
    let polices = Polices::charger();
    let compositeur = Compositeur::new(&m, &polices);
    let voix_montage = Montage { sous_titres: m.sous_titres.clone(), reglages: m.reglages.clone(), ..Default::default() };
    let voix = Compositeur::new(&voix_montage, &polices);
    // Peu de fils et de places : chaque image pèse 9 Mo, la mémoire doit rester bornée.
    let fils = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(2, 12);
    let (envoi, recu): (SyncSender<(u64, Image)>, Receiver<(u64, Image)>) = sync_channel(fils);
    let recu = Mutex::new(recu);
    let (fait_envoi, fait_recu) = sync_channel::<(u64, Option<Vec<u8>>)>(fils);
    let lecture = Lecture { video: video.to_path_buf(), dossier, montage: m.clone(), ips_texte, ips, l, h, lo, ho };

    let mut resultat: Result<(), Erreur> = Ok(());
    let lu = std::thread::scope(|portee| {
        let lecteur = portee.spawn(|| lecture.derouler(envoi, suivi));
        for _ in 0..fils {
            let (recu, fait, c, v) = (&recu, fait_envoi.clone(), &compositeur, &voix);
            portee.spawn(move || {
                loop {
                    let suivant = recu.lock().ok().and_then(|r| r.recv().ok());
                    let Some((n, image)) = suivant else { return };
                    if fait.send((n, peindre(image, c, v, l, h, lo, ho))).is_err() {
                        return;
                    }
                }
            });
        }
        drop(fait_envoi);
        // Les images reviennent dans le désordre : on les remet en rang avant d'encoder.
        let mut attente: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
        let mut suivante = 0u64;
        for (n, image) in fait_recu.iter() {
            let Some(image) = image else {
                resultat = Err(Erreur("une image n'a pas pu être peinte".into()));
                suivi.arret.store(true, Ordering::Relaxed);
                continue;
            };
            attente.insert(n, image);
            while let Some(i) = attente.remove(&suivante) {
                if limite.is_some_and(|l| suivante >= l) {
                    suivi.assez.store(true, Ordering::Relaxed);
                } else if resultat.is_ok() && entree.write_all(&i).is_err() {
                    resultat = Err(Erreur("l'encodeur s'est arrêté".into()));
                    suivi.arret.store(true, Ordering::Relaxed);
                }
                suivante += 1;
                suivi.faites.store(limite.map_or(suivante, |l| suivante.min(l)) as u32, Ordering::Relaxed);
            }
        }
        lecteur.join().unwrap_or_else(|_| Err(Erreur("la lecture s'est arrêtée".into())))
    });
    drop(entree);
    let code = encodeur.wait().map_err(|e| Erreur(e.to_string()))?;
    let bavard = erreurs.join().unwrap_or_default();
    let abandon = suivi.arret.load(Ordering::Relaxed);
    if abandon || resultat.is_err() || lu.is_err() || !code.success() {
        let _ = std::fs::remove_file(&partiel);
        lu?;
        resultat?;
        return Err(Erreur(if abandon {
            "Rendu arrêté.".into()
        } else {
            format!("L'encodage a échoué : {}", bavard.lines().last().unwrap_or("ffmpeg"))
        }));
    }
    std::fs::rename(&partiel, sortie).map_err(|e| Erreur(format!("{} : {e}", sortie.display())))
}

/// Toute la sortie d'une prise : `<vidéo> montage YouTube.mp4`, les sous-titres à part s'il y en a,
/// puis, si la vignette est là, `<vidéo> avec vignette.mp4`. Rend un compte en français.
pub fn monter(video: &Path, montage: &Montage, voix: &Voix, pre: &Prereglage, suivi: &Suivi) -> Result<String, Erreur> {
    let rendu = noyau::chemin_derive(video, pre.suffixe);
    rendre(video, montage, voix, pre, &rendu, suivi)?;
    let mut compte = format!("Rendu : {} ({:.0} Mo)", nom(&rendu), taille_mo(&rendu));
    if !pre.youtube {
        return Ok(compte + ".");
    }
    ecrire_srt(&noyau::chemin_derive(video, noyau::SUFFIXE_SRT_MONTAGE), montage, 0.0, &mut compte)?;
    let png = noyau::chemin_derive(video, noyau::SUFFIXE_PNG);
    if !png.exists() {
        compte.push_str(". Pas de vignette écrite : pas de version avec vignette");
        return Ok(compte + ".");
    }
    suivi.vignette.store(true, Ordering::Relaxed);
    let montee = noyau::chemin_derive(video, noyau::SUFFIXE_MONTEE);
    let partiel = videos::chemin_partiel(&montee);
    let infos = sonder(&rendu)?;
    let mut encodeur = lancer(&videos::commande_montage(&rendu, &png, &partiel, &infos), true)?;
    drop(encodeur.stdin.take());
    while encodeur.try_wait().map_err(|e| Erreur(e.to_string()))?.is_none() {
        if suivi.arret.load(Ordering::Relaxed) {
            let _ = encodeur.kill();
            let _ = encodeur.wait();
            let _ = std::fs::remove_file(&partiel);
            return Err(Erreur("Rendu arrêté.".into()));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let fin = encodeur.wait_with_output().map_err(|e| Erreur(e.to_string()))?;
    if !fin.status.success() {
        let _ = std::fs::remove_file(&partiel);
        return Err(Erreur(format!("Vignette non collée : {}", String::from_utf8_lossy(&fin.stderr).lines().last().unwrap_or("ffmpeg"))));
    }
    std::fs::rename(&partiel, &montee).map_err(|e| Erreur(e.to_string()))?;
    ecrire_srt(&noyau::chemin_derive(video, noyau::SUFFIXE_SRT), montage, noyau::DUREE_VIGNETTE, &mut compte)?;
    compte.push_str(&format!(" ; {} ({:.0} Mo)", nom(&montee), taille_mo(&montee)));
    Ok(compte + ".")
}

fn nom(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn taille_mo(p: &Path) -> f64 {
    std::fs::metadata(p).map_or(0.0, |m| m.len() as f64 / 1e6)
}

/// Les sous-titres à part, à côté de la vidéo ; incrustés, un .srt resté d'avant s'efface (ils
/// seraient en double).
fn ecrire_srt(chemin: &Path, montage: &Montage, decalage: f64, compte: &mut String) -> Result<(), Erreur> {
    if montage.sous_titres_a_part() {
        let n = noyau::sous_titres::ecrire_srt(chemin, montage, decalage).map_err(|e| Erreur(format!("{} : {e}", chemin.display())))?;
        if decalage == 0.0 {
            compte.push_str(&format!(" ; {n} sous-titre(s) à part"));
        }
    } else if chemin.exists() {
        std::fs::remove_file(chemin).map_err(|e| Erreur(format!("{} : {e}", chemin.display())))?;
    }
    Ok(())
}

pub fn sonder(video: &Path) -> Result<Infos, Erreur> {
    let cmd = videos::commande_sonder(video);
    let sortie = Command::new(&cmd[0]).args(&cmd[1..]).output().map_err(|e| Erreur(format!("ffprobe : {e}")))?;
    videos::lire_infos(&String::from_utf8_lossy(&sortie.stdout))
}

#[cfg(test)]
mod tests_volume {
    use super::*;

    /// Un son trop faible, un bout seulement : l'écoute sort au niveau visé, sans dépasser la crête.
    #[test]
    fn avant_apres_au_niveau() {
        let d = std::env::temp_dir().join(format!("avant-apres-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let v = d.join("son.mp4");
        let fait = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=f=440:r=48000:d=12,volume=-20dB", "-c:a", "aac"])
            .arg(&v)
            .status()
            .unwrap();
        assert!(fait.success());
        let (a, b) = avant_apres(&v, 1.0, 10.0, &Voix::default()).unwrap();
        let attendu = 48000 * 2 * 2 * 10;
        assert!(a.pcm.len().abs_diff(attendu) < 4096 && b.pcm.len().abs_diff(attendu) < 4096, "{} {}", a.pcm.len(), b.pcm.len());
        let (la, lb) = (a.lufs.unwrap(), b.lufs.unwrap());
        assert!(la < -30.0 && (lb + 14.0).abs() < 1.5, "{la} → {lb}");
        let pic = b.pcm.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]).unsigned_abs()).max().unwrap() as f64 / 32768.0;
        assert!(pic <= 10f64.powf(-1.0 / 20.0), "{pic}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn mesure_lue_ou_ecartee() {
        let bloc = |i: &str| format!("[Parsed_loudnorm_0 @ 0x1]\n{{\n\t\"input_i\" : \"{i}\",\n\t\"input_tp\" : \"-9.40\",\n\t\"input_lra\" : \"7.40\",\n\t\"input_thresh\" : \"-41.00\",\n\t\"target_offset\" : \"0.20\"\n}}\n");
        let v = Voix::default();
        let f = filtre_depuis_mesure(&bloc("-30.90"), &v).unwrap();
        assert_eq!(f, "volume=16.90dB,alimiter=limit=0.8414:attack=5:release=50:level=false");
        assert_eq!(filtre_depuis_mesure(&bloc("-inf"), &v), None);
        assert_eq!(filtre_depuis_mesure(&bloc("-70.00"), &v), None);
        assert_eq!(filtre_depuis_mesure("rien", &v), None);
    }
}
