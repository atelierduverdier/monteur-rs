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
use std::process::{Child, ChildStdout, Stdio};
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
    /// Avant les images : DeepFilterNet débruite la voix.
    pub debruitage: AtomicBool,
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

/// La webcam à partir de sa seconde `debut`, à la cadence de la sortie et à la taille `l × h`.
pub fn commande_webcam(chemin: &Path, debut: f64, l: u32, h: u32, ips: &str) -> Vec<String> {
    vec![
        s("ffmpeg"), s("-v"), s("error"), s("-ss"), format!("{debut:.3}"), s("-i"), chemin.to_string_lossy().into_owned(),
        s("-an"), s("-vf"), format!("fps={ips},scale={l}:{h}"), s("-f"), s("rawvideo"), s("-pix_fmt"), s("rgba"), s("-"),
    ]
}

/// La taille où lire la webcam : la sienne, ramenée à 1280 de large au plus (assez pour toute l'image en grand).
fn taille_webcam(l: i64, h: i64) -> (u32, u32) {
    let (l, h) = (l.max(2) as f64, h.max(2) as f64);
    let k = (1280.0 / l).min(1.0);
    let pair = |x: f64| ((x * k / 2.0).round() as u32).max(1) * 2;
    (pair(l), pair(h))
}

/// Le volume autour de chaque fondu : il descend avant la coupe et remonte après, le temps du fondu
/// d'image. Posé avant le retrait des coupes : les temps sont ceux de la prise.
fn enveloppe_son(fondus: &[(f64, f64, f64)]) -> String {
    if fondus.is_empty() {
        return String::new();
    }
    let termes: Vec<String> = fondus
        .iter()
        .flat_map(|&(a, b, d)| {
            let h = d / 2.0;
            [
                format!("(1-between(t,{:.4},{a:.4})*(t-{:.4})/{h:.4})", a - h, a - h),
                format!("(1-between(t,{b:.4},{:.4})*({:.4}-t)/{h:.4})", b + h, b + h),
            ]
        })
        .collect();
    format!("asetnsamples=n=256:p=0,volume='{}':eval=frame,", en_arbre(&termes, '*'))
}

/// L'expression ffmpeg vraie dans les passages à retirer.
pub fn expression_coupes(coupes: &[(f64, f64)]) -> Option<String> {
    let termes: Vec<String> = coupes.iter().map(|(a, b)| format!("between(t,{a:.4},{b:.4})")).collect();
    (!termes.is_empty()).then(|| en_arbre(&termes, '+'))
}

/// Les termes liés deux à deux, entre parenthèses : ffmpeg refuse une chaîne de plus d'une
/// centaine d'opérations à la file (« Error when evaluating the volume expression »), et 76 coupes
/// fondues en font 152 (Christophe, 25/11/2026 : 40 min de prise, rendu en échec dès le départ).
/// En arbre, la profondeur reste celle du logarithme : 1 000 termes passent.
fn en_arbre(termes: &[String], op: char) -> String {
    match termes {
        [] => String::new(),
        [seul] => seul.clone(),
        _ => {
            let (a, b) = termes.split_at(termes.len() / 2);
            format!("({}{op}{})", en_arbre(a, op), en_arbre(b, op))
        }
    }
}

/// Une vidéo intercalée : son fichier et a-t-elle du son ?
pub struct ClipSon<'a> {
    pub insertion: &'a Insertion,
    pub chemin: PathBuf,
    pub son: bool,
}

/// Le graphe du son. Sans intercalée, celui du Monteur 1. Avec : la prise est découpée aux instants
/// des intercalées, et leur son (ou un silence) se glisse entre les morceaux.
fn graphe_son(retire: &[(f64, f64)], clips: &[ClipSon], fondus: &[(f64, f64, f64)]) -> String {
    let enveloppe = enveloppe_son(fondus);
    let expr = expression_coupes(retire);
    let selection = expr.as_ref().map(|e| format!("aselect='not({e})',asetpts=N/SR/TB")).unwrap_or_else(|| s("asetpts=N/SR/TB"));
    if clips.is_empty() {
        let f = expr.map(|e| format!("aselect='not({e})',asetpts=N/SR/TB")).unwrap_or_else(|| s("anull"));
        return format!("[1:a]{enveloppe}{f}[a]");
    }
    const FORMAT: &str = "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo";
    let n = clips.len() + 1;
    let mut g: Vec<String> = Vec::new();
    let sorties: String = (0..n).map(|i| format!("[p{i}]")).collect();
    g.push(format!("[1:a]{enveloppe}asplit={n}{sorties}"));
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
/// `voix` : le son de la prise, ou son WAV débruité (mêmes instants).
pub fn commande_encodage(voix: &Path, sortie: &Path, l: u32, h: u32, ips: &str, retire: &[(f64, f64)], son: bool, clips: &[ClipSon], fondus: &[(f64, f64, f64)], volume: Option<&str>, pre: &Prereglage) -> Vec<String> {
    let mut cmd = vec![
        s("ffmpeg"), s("-v"), s("error"), s("-y"), s("-nostats"), s("-f"), s("rawvideo"), s("-pix_fmt"), s("rgba"),
        s("-s"), format!("{l}x{h}"), s("-r"), s(ips), s("-i"), s("-"),
    ];
    if son {
        cmd.extend([s("-i"), voix.to_string_lossy().into_owned()]);
        for c in clips.iter().filter(|c| c.son) {
            cmd.extend([s("-i"), c.chemin.to_string_lossy().into_owned()]);
        }
        let (graphe, sortie_son) = match volume {
            Some(v) => (format!("{};[a]{v}[fort]", graphe_son(retire, clips, fondus)), "[fort]"),
            None => (graphe_son(retire, clips, fondus), "[a]"),
        };
        cmd.extend([s("-filter_complex"), graphe, s("-map"), s("0:v"), s("-map"), s(sortie_son)]);
        // En stéréo : le WAV débruité est mono, et une voix mono passe à l'identique dans les deux canaux.
        cmd.extend([s("-ac"), s("2"), s("-c:a"), s("aac"), s("-b:a"), s("192k"), s("-shortest")]);
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
/// `loudnorm`, repris au second). `None` : rien à appliquer. `debruite` : `son` est le WAV de DeepFilterNet.
fn filtre_son(son: &Path, debruite: bool, retire: &[(f64, f64)], clips: &[ClipSon], fondus: &[(f64, f64, f64)], voix: &Voix) -> Option<String> {
    let nettoyage = voix.filtres(debruite);
    if !voix.niveau {
        return nettoyage;
    }
    let mut cmd = vec![s("-hide_banner"), s("-nostats"), s("-f"), s("lavfi"), s("-i"), s("nullsrc=s=2x2:d=0.04"), s("-i"), son.to_string_lossy().into_owned()];
    for c in clips.iter().filter(|c| c.son) {
        cmd.extend([s("-i"), c.chemin.to_string_lossy().into_owned()]);
    }
    let avant = nettoyage.as_ref().map(|f| format!("{f},")).unwrap_or_default();
    let graphe = format!("{};[a]{avant}loudnorm={}:print_format=json[mesure]", graphe_son(retire, clips, fondus), voix.cible());
    cmd.extend([s("-filter_complex"), graphe, s("-map"), s("[mesure]"), s("-f"), s("null"), s("-")]);
    let sortie = noyau::systeme::commande("ffmpeg").args(&cmd).stdin(Stdio::null()).output().ok();
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
    let mut c = noyau::systeme::commande("ffmpeg");
    c.args(["-hide_banner", "-nostats", "-ss", &format!("{debut:.3}"), "-t", &format!("{duree:.3}"), "-i"]).arg(video).arg("-vn");
    if let Some(f) = filtre {
        c.args(["-af", f]);
    }
    c.args(sortie).stdin(Stdio::null()).output().ok()
}

/// Un WAV mono en flottants : l'en-tête d'abord, ses tailles réécrites à la fin.
fn entete_wav(echantillons: u32) -> [u8; 44] {
    let octets = echantillons.saturating_mul(4);
    let mut e = [0u8; 44];
    let mut i = 0;
    let mut mettre = |b: &[u8]| {
        e[i..i + b.len()].copy_from_slice(b);
        i += b.len();
    };
    mettre(b"RIFF");
    mettre(&octets.saturating_add(36).to_le_bytes());
    mettre(b"WAVEfmt ");
    mettre(&16u32.to_le_bytes());
    mettre(&3u16.to_le_bytes()); // flottants IEEE
    mettre(&1u16.to_le_bytes());
    mettre(&debruitage::TAUX.to_le_bytes());
    mettre(&(debruitage::TAUX * 4).to_le_bytes());
    mettre(&4u16.to_le_bytes());
    mettre(&32u16.to_le_bytes());
    mettre(b"data");
    mettre(&octets.to_le_bytes());
    e
}

/// Le son de la prise (ou d'un passage `(début, durée)`) débruité par DeepFilterNet, écrit dans `wav`
/// en mono à 48 kHz, calé sur la prise. Les deux canaux sont moyennés : une voix mono enregistrée en
/// stéréo garde son niveau. Une prise de 40 min pèse 450 Mo de WAV, le temps du rendu.
pub fn debruiter_son(video: &Path, passage: Option<(f64, f64)>, force: f64, wav: &Path, suivi: &Suivi) -> Result<(), Erreur> {
    let rate = |e: String| Erreur(format!("Débruitage : {e}"));
    let mut d = debruitage::Debruiteur::new(debruitage::attenuation_db(force)).map_err(rate)?;
    let mut c = noyau::systeme::commande("ffmpeg");
    c.args(["-v", "error", "-nostdin"]);
    if let Some((debut, duree)) = passage {
        c.args(["-ss", &format!("{debut:.3}"), "-t", &format!("{duree:.3}")]);
    }
    c.arg("-i").arg(video).args(["-vn", "-ac", "2", "-ar", &debruitage::TAUX.to_string(), "-f", "f32le", "-"]);
    let mut lecteur = c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| rate(format!("ffmpeg : {e}")))?;
    let mut flux = lecteur.stdout.take().expect("sortie de ffmpeg");
    let erreurs = lecteur.stderr.take();
    let erreurs = std::thread::spawn(move || {
        let mut t = String::new();
        if let Some(mut e) = erreurs {
            let _ = e.read_to_string(&mut t);
        }
        t
    });
    let fichier = std::fs::File::create(wav).map_err(|e| rate(format!("{} : {e}", wav.display())))?;
    let mut ecrit = std::io::BufWriter::new(fichier);
    let mut resultat = ecrit.write_all(&entete_wav(0)).map_err(|e| rate(e.to_string()));
    // Une seconde de son à la fois : 384 ko lus, 192 ko écrits.
    let mut brut = vec![0u8; debruitage::TAUX as usize * 8];
    let (mut plein, mut total) = (0usize, 0u64);
    let mut net = Vec::new();
    let ecrire = |net: &mut Vec<f32>, ecrit: &mut std::io::BufWriter<std::fs::File>| -> std::io::Result<()> {
        for x in net.drain(..) {
            ecrit.write_all(&x.to_le_bytes())?;
        }
        Ok(())
    };
    while resultat.is_ok() {
        if suivi.arret.load(Ordering::Relaxed) {
            resultat = Err(Erreur("Rendu arrêté.".into()));
            break;
        }
        let lu = match flux.read(&mut brut[plein..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                resultat = Err(rate(e.to_string()));
                break;
            }
        };
        plein += lu;
        let entiers = plein / 8 * 8;
        let mono: Vec<f32> = brut[..entiers]
            .chunks_exact(8)
            .map(|o| (f32::from_le_bytes([o[0], o[1], o[2], o[3]]) + f32::from_le_bytes([o[4], o[5], o[6], o[7]])) / 2.0)
            .collect();
        brut.copy_within(entiers..plein, 0);
        plein -= entiers;
        total += mono.len() as u64;
        resultat = d.pousser(&mono, &mut net).map_err(rate).and_then(|()| ecrire(&mut net, &mut ecrit).map_err(|e| rate(e.to_string())));
    }
    drop(flux);
    if resultat.is_err() {
        let _ = lecteur.kill();
    }
    let code = lecteur.wait().map_err(|e| rate(e.to_string()))?;
    let bavard = erreurs.join().unwrap_or_default();
    if resultat.is_ok() && !code.success() {
        resultat = Err(rate(format!("lecture du son : {}", cause_ffmpeg(&bavard))));
    }
    if resultat.is_ok() && total == 0 {
        resultat = Err(rate("la prise n'a pas de son".into()));
    }
    let resultat = resultat.and_then(|()| {
        d.finir(&mut net).map_err(rate)?;
        ecrire(&mut net, &mut ecrit).map_err(|e| rate(e.to_string()))?;
        let mut fichier = ecrit.into_inner().map_err(|e| rate(e.to_string()))?;
        use std::io::Seek;
        fichier.seek(std::io::SeekFrom::Start(0)).and_then(|_| fichier.write_all(&entete_wav(total.min(u32::MAX as u64 / 4 - 9) as u32))).map_err(|e| rate(e.to_string()))
    });
    if resultat.is_err() {
        let _ = std::fs::remove_file(wav);
    }
    resultat
}

/// Un fichier de passage, effacé quoi qu'il arrive.
struct Efface(PathBuf);

impl Drop for Efface {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Le WAV débruité d'un rendu, à côté de son `.partiel`.
fn chemin_voix_debruitee(partiel: &Path) -> PathBuf {
    let mut nom = partiel.as_os_str().to_owned();
    nom.push(".voix.wav");
    PathBuf::from(nom)
}

/// Le passage tel quel, puis nettoyé comme au rendu : débruité par DeepFilterNet dans un WAV jetable,
/// ou par `afftdn` en secours.
pub fn avant_apres(video: &Path, debut: f64, duree: f64, voix: &Voix) -> Option<(Extrait, Extrait)> {
    let wav = std::env::temp_dir().join(format!("monteur-voix-{}-{}.wav", std::process::id(), PASSAGES.fetch_add(1, Ordering::Relaxed)));
    let debruite = voix.bruit && debruiter_son(video, Some((debut, duree)), voix.bruit_force, &wav, &Suivi::default()).is_ok();
    let r = avant_apres_depuis(video, debut, duree, voix, debruite.then_some(wav.as_path()));
    if debruite {
        let _ = std::fs::remove_file(&wav);
    }
    r
}

/// Pour que deux écoutes calculées en même temps n'écrivent pas le même WAV.
static PASSAGES: AtomicU32 = AtomicU32::new(0);

/// `debruite` : le passage déjà débruité, qui commence à 0.
fn avant_apres_depuis(video: &Path, debut: f64, duree: f64, voix: &Voix, debruite: Option<&Path>) -> Option<(Extrait, Extrait)> {
    const PCM: [&str; 9] = ["-ac", "2", "-ar", "48000", "-f", "s16le", "-c:a", "pcm_s16le", "-"];
    const NUL: [&str; 3] = ["-f", "null", "-"];
    let (source, depart) = debruite.map_or((video, debut), |w| (w, 0.0));
    let mesure = |v: &Path, d: f64, f: &str| ffmpeg_extrait(v, d, duree, Some(f), &NUL).map(|o| String::from_utf8_lossy(&o.stderr).into_owned());
    let vers_cible = format!("loudnorm={}:print_format=json", voix.cible());
    let brut = mesure(video, debut, &vers_cible)?;
    let nettoyage = voix.filtres(debruite.is_some());
    let avant_niveau = nettoyage.as_ref().map(|f| format!("{f},")).unwrap_or_default();
    let filtre = if voix.niveau {
        let m = mesure(source, depart, &format!("{avant_niveau}{vers_cible}"))?;
        filtre_depuis_mesure(&m, voix).map(|n| format!("{avant_niveau}{n}")).or(nettoyage)
    } else {
        nettoyage
    };
    let lire = |v: &Path, d: f64, f: Option<&str>| ffmpeg_extrait(v, d, duree, f, &PCM).filter(|o| o.status.success()).map(|o| o.stdout);
    let (pcm_avant, pcm_apres) = (lire(video, debut, None)?, lire(source, depart, filtre.as_deref())?);
    // Mesurée sur ce qui sort vraiment, pas sur la cible.
    let lufs = |f: Option<&str>| {
        let texte = match f {
            Some(f) => format!("{f},{}", vers_cible),
            None => vers_cible.clone(),
        };
        mesure(source, depart, &texte).and_then(|m| lire_mesure(&m)).map(|m| m[0])
    };
    Some((Extrait { pcm: pcm_avant, lufs: lire_mesure(&brut).map(|m| m[0]) }, Extrait { pcm: pcm_apres, lufs: lufs(filtre.as_deref()) }))
}

fn lancer(cmd: &[String], entree: bool) -> Result<Child, Erreur> {
    noyau::systeme::commande(&cmd[0])
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

/// Une image de la webcam, à la taille où on la lit.
type Camera = Option<Vec<u8>>;

enum Image {
    /// Une image de la prise, à peindre à l'instant t, avec celle de la webcam s'il y en a une.
    Prise { t: f64, rgba: Vec<u8>, camera: Camera },
    /// Par-dessus : déjà à la taille de sortie ; seuls les sous-titres s'y posent.
    Dessus { t: f64, rgba: Vec<u8> },
    /// Intercalée : elle sort telle quelle.
    Clip { rgba: Vec<u8> },
    /// Un fondu : deux images de la prise, à peindre chacune à son instant, puis à mêler.
    Fondu { avant: (f64, Vec<u8>, Camera), apres: (f64, Vec<u8>, Camera), poids: f32 },
}

/// Un fondu en images de la prise : `h` images de chaque côté du début et de la fin de la coupe.
#[derive(Clone, Copy)]
struct FonduImages {
    debut: u64,
    fin: u64,
    h: u64,
}

impl FonduImages {
    fn de(a: f64, b: f64, d: f64, ips: f64) -> Self {
        Self { debut: (a * ips).ceil() as u64, fin: (b * ips).ceil() as u64, h: ((d * ips / 2.0).round() as u64).max(1) }
    }
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
    /// La webcam à incruster : son fichier, et la taille où on la lit.
    webcam: Option<(PathBuf, u32, u32)>,
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
        // Les images qui finissent le passage d'avant attendent celles qui commencent le passage
        // d'après : la prise se lit dans l'ordre. Au plus la durée d'un fondu : la mémoire reste bornée.
        let fondus: Vec<FonduImages> = self.montage.fondus().iter().map(|&(a, b, d)| FonduImages::de(a, b, d, self.ips)).collect();
        let mut sortantes: std::collections::VecDeque<(f64, Vec<u8>, Camera)> = std::collections::VecDeque::new();
        // La webcam : une image par image de la prise dès qu'elle tourne, gardée ou non, pour rester calée à travers
        // les coupes. Elle avait commencé avant la prise (decalage > 0) : on saute ce qui précède.
        let decalage = self.montage.webcam.as_ref().map_or(0.0, |w| w.decalage);
        let mut camera: Option<(Child, ChildStdout, usize)> = match &self.webcam {
            Some((chemin, cl, ch)) => {
                let mut c = lancer(&commande_webcam(chemin, decalage.max(0.0), *cl, *ch, &self.ips_texte), false)?;
                let f = c.stdout.take().expect("sortie de la webcam");
                Some((c, f, *cl as usize * *ch as usize * 4))
            }
            None => None,
        };
        while !suivi.fin_de_lecture() {
            let Some(rgba) = lire_image(&mut flux, l * h * 4) else { break };
            let t = n as f64 / self.ips;
            n += 1;
            lues += 1;
            let image_camera = match &mut camera {
                Some((_, f, taille)) if t + decalage >= -1e-6 => lire_image(f, *taille),
                _ => None,
            };
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
            let rang = n - 1;
            if fondus.iter().any(|f| rang + f.h >= f.debut && rang < f.debut + f.h) {
                sortantes.push_back((t, rgba, image_camera));
                continue;
            }
            if let Some(f) = fondus.iter().find(|f| rang + f.h >= f.fin && rang < f.fin + f.h)
                && let Some(avant) = sortantes.pop_front()
            {
                let k = rang + f.h - f.fin;
                let poids = (k + 1) as f32 / (2 * f.h + 1) as f32;
                if envoi.send((ordre, Image::Fondu { avant, apres: (t, rgba, image_camera), poids })).is_err() {
                    break;
                }
                ordre += 1;
                continue;
            }
            if !image_gardee(&self.montage, t, self.ips) {
                continue;
            }
            let image = match remplace {
                Some(rgba) => Image::Dessus { t, rgba },
                None => Image::Prise { t, rgba, camera: image_camera },
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
        if let Some((mut c, f, _)) = camera {
            drop(f);
            let _ = c.kill();
            let _ = c.wait();
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
#[allow(clippy::too_many_arguments)]
fn peindre(image: Image, c: &Compositeur, voix: &Compositeur, l: u32, h: u32, lo: u32, ho: u32, cam: (u32, u32)) -> Option<Vec<u8>> {
    let cible = R::new(0.0, 0.0, lo as f64, ho as f64);
    let prise = |t: f64, rgba: Vec<u8>, camera: Camera| {
        let source = Pixmap::from_vec(rgba, IntSize::from_wh(l, h)?)?;
        let camera = camera.and_then(|rgba| Pixmap::from_vec(rgba, IntSize::from_wh(cam.0, cam.1)?));
        let mut p = Pixmap::new(lo, ho)?;
        c.peindre_avec_webcam(&mut p.as_mut(), Some(source.as_ref()), camera.as_ref().map(Pixmap::as_ref), t, cible, false);
        Some(p.take())
    };
    match image {
        Image::Clip { rgba } => Some(rgba),
        Image::Fondu { avant, apres, poids } => {
            let (mut a, b) = (prise(avant.0, avant.1, avant.2)?, prise(apres.0, apres.1, apres.2)?);
            let q = (poids * 256.0) as u32;
            for (x, y) in a.iter_mut().zip(&b) {
                *x = ((*x as u32 * (256 - q) + *y as u32 * q) >> 8) as u8;
            }
            Some(a)
        }
        Image::Dessus { t, rgba } => {
            let mut p = Pixmap::from_vec(rgba, IntSize::from_wh(lo, ho)?)?;
            voix.peindre_calque(&mut p.as_mut(), t, cible, (0.0, 0.0, lo as f64, ho as f64), lo as f64, ho as f64);
            Some(p.take())
        }
        Image::Prise { t, rgba, camera } => prise(t, rgba, camera),
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
    // La webcam montrée, si son fichier se lit ; sinon le rendu se fait sans elle, comme l'aperçu.
    let webcam = montage.webcam.as_ref().filter(|w| w.montrer).and_then(|w| {
        let chemin = dossier.join(&w.video);
        let infos = sonder(&chemin).ok()?;
        let (cl, ch) = taille_webcam(infos.largeur, infos.hauteur);
        Some((chemin, cl, ch))
    });
    let cam = webcam.as_ref().map_or((0, 0), |w| (w.1, w.2));
    let son = infos.son && pre.son;
    let fondus = montage.fondus();
    // La voix débruitée par DeepFilterNet, à côté du rendu, le temps du rendu ; s'il échoue, afftdn le
    // remplace dans la chaîne.
    let wav = Efface(chemin_voix_debruitee(&partiel));
    let debruite = son && voix.bruit && {
        suivi.debruitage.store(true, Ordering::Relaxed);
        let r = debruiter_son(video, None, voix.bruit_force, &wav.0, suivi);
        suivi.debruitage.store(false, Ordering::Relaxed);
        match r {
            Ok(()) => true,
            Err(e) if suivi.arret.load(Ordering::Relaxed) => return Err(e),
            Err(e) => {
                eprintln!("{} : afftdn en secours", e.0);
                false
            }
        }
    };
    let source_son = if debruite { wav.0.as_path() } else { video };
    let volume = if son { filtre_son(source_son, debruite, &retire, &clips, &fondus, voix) } else { None };
    let mut encodeur = lancer(&commande_encodage(source_son, &partiel, lo, ho, &ips_texte, &retire, son, &clips, &fondus, volume.as_deref(), pre), true)?;
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
    let lecture = Lecture { video: video.to_path_buf(), dossier, montage: m.clone(), ips_texte, ips, l, h, lo, ho, webcam };

    let mut resultat: Result<(), Erreur> = Ok(());
    let lu = std::thread::scope(|portee| {
        let lecteur = portee.spawn(|| lecture.derouler(envoi, suivi));
        for _ in 0..fils {
            let (recu, fait, c, v) = (&recu, fait_envoi.clone(), &compositeur, &voix);
            portee.spawn(move || {
                loop {
                    let suivant = recu.lock().ok().and_then(|r| r.recv().ok());
                    let Some((n, image)) = suivant else { return };
                    if fait.send((n, peindre(image, c, v, l, h, lo, ho, cam))).is_err() {
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
        // L'encodeur qui refuse passe d'abord : sinon il ne restait que « l'encodeur s'est arrêté ».
        if !code.success() {
            return Err(Erreur(format!("L'encodage a échoué : {}", cause_ffmpeg(&bavard))));
        }
        lu?;
        resultat?;
        return Err(Erreur("Rendu arrêté.".into()));
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
    // Le carton « Fait avec VerdierCAM », peint pour l'occasion et effacé après.
    let carton = montage.reglages.carton_fin.then(|| noyau::chemin_derive(video, noyau::SUFFIXE_CARTON));
    if let Some(c) = &carton {
        composeur::vignette::carton_de_fin(&Polices::charger()).save_png(c).map_err(|e| Erreur(format!("Carton de fin non peint : {e}")))?;
    }
    let commande = videos::commande_montage(&rendu, &png, &partiel, &infos, carton.as_deref());
    let encodeur = lancer(&commande, true);
    let efface_carton = || {
        if let Some(c) = &carton {
            let _ = std::fs::remove_file(c);
        }
    };
    let mut encodeur = encodeur.inspect_err(|_| efface_carton())?;
    drop(encodeur.stdin.take());
    while encodeur.try_wait().map_err(|e| Erreur(e.to_string()))?.is_none() {
        if suivi.arret.load(Ordering::Relaxed) {
            let _ = encodeur.kill();
            let _ = encodeur.wait();
            let _ = std::fs::remove_file(&partiel);
            efface_carton();
            return Err(Erreur("Rendu arrêté.".into()));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let fin = encodeur.wait_with_output().map_err(|e| Erreur(e.to_string()));
    efface_carton();
    let fin = fin?;
    if !fin.status.success() {
        let _ = std::fs::remove_file(&partiel);
        return Err(Erreur(format!("Vignette non collée : {}", cause_ffmpeg(&String::from_utf8_lossy(&fin.stderr)))));
    }
    std::fs::rename(&partiel, &montee).map_err(|e| Erreur(e.to_string()))?;
    ecrire_srt(&noyau::chemin_derive(video, noyau::SUFFIXE_SRT), montage, noyau::DUREE_VIGNETTE, &mut compte)?;
    compte.push_str(&format!(" ; {} ({:.0} Mo)", nom(&montee), taille_mo(&montee)));
    Ok(compte + ".")
}

/// Ce que ffmpeg dit de son échec, en une ligne : sa première erreur et sa dernière, sans le
/// « [filtre @ 0x…] » devant, chacune coupée à 160 caractères (une expression fait des milliers).
pub fn cause_ffmpeg(bavard: &str) -> String {
    let lisible = |l: &str| {
        let l = match l.strip_prefix('[').and_then(|r| r.split_once("] ")) {
            Some((tete, reste)) if tete.contains(" @ ") => reste,
            _ => l,
        };
        if l.chars().count() > 160 { l.chars().take(159).collect::<String>() + "…" } else { l.to_string() }
    };
    let lignes: Vec<String> = bavard.lines().map(str::trim).filter(|l| !l.is_empty()).map(lisible).collect();
    match lignes.as_slice() {
        [] => "ffmpeg s'est arrêté sans rien dire".into(),
        [seule] => seule.clone(),
        [premiere, .., derniere] => format!("{premiere} — {derniere}"),
    }
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
    let sortie = noyau::systeme::commande(&cmd[0]).args(&cmd[1..]).output().map_err(|e| Erreur(format!("ffprobe : {e}")))?;
    videos::lire_infos(&String::from_utf8_lossy(&sortie.stdout))
}

#[cfg(test)]
mod tests_volume {
    use super::*;
    use std::process::Command;

    /// Une coupe entre un plan rouge et un plan bleu : mêmes images au total, et les deux couleurs
    /// mêlées au raccord.
    #[test]
    fn fondu_au_raccord() {
        let d = std::env::temp_dir().join(format!("fondu-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (v, s) = (d.join("plans.mp4"), d.join("sortie.mp4"));
        let fait = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=red:s=64x48:r=30:d=2.5", "-f", "lavfi", "-i", "color=blue:s=64x48:r=30:d=2.5"])
            .args(["-f", "lavfi", "-i", "sine=f=440:r=48000:d=5"])
            .args(["-filter_complex", "[0:v][1:v]concat=n=2:v=1[v]", "-map", "[v]", "-map", "2:a", "-c:v", "libx264", "-crf", "1", "-pix_fmt", "yuv420p", "-c:a", "aac"])
            .arg(&v)
            .status()
            .unwrap();
        assert!(fait.success());
        let mut m = Montage { duree: 5.0, largeur: 64, hauteur: 48, ..Montage::default() };
        m.coupes.push(noyau::montage::Coupe { debut: 2.0, fin: 3.0, active: true, origine: "main".into(), vitesse: 0.0, autres: Default::default() });
        let suivi = Suivi::default();
        rendre(&v, &m, &Voix::aucune(), &noyau::prereglages::PREREGLAGES[0], &s, &suivi).unwrap();
        assert_eq!(suivi.faites.load(Ordering::Relaxed), 120);
        let couleur = |t: f64| {
            let o = Command::new("ffmpeg")
                .args(["-v", "error", "-ss", &format!("{t}"), "-i"])
                .arg(&s)
                .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
                .output()
                .unwrap();
            (o.stdout[3000] as i32, o.stdout[3002] as i32)
        };
        let (r, b) = couleur(1.0);
        assert!(r > 200 && b < 40, "avant : {r} {b}");
        let (r, b) = couleur(3.0);
        assert!(r < 40 && b > 200, "après : {r} {b}");
        let (r, b) = couleur(2.0);
        assert!((80..180).contains(&r) && (80..180).contains(&b), "au raccord : {r} {b}");
        std::fs::remove_dir_all(&d).unwrap();
    }

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
        // Un son pur : DeepFilterNet le prendrait pour du bruit, la chaîne sans lui se mesure ici.
        let (a, b) = avant_apres(&v, 1.0, 10.0, &Voix { bruit: false, ..Voix::default() }).unwrap();
        let attendu = 48000 * 2 * 2 * 10;
        assert!(a.pcm.len().abs_diff(attendu) < 4096 && b.pcm.len().abs_diff(attendu) < 4096, "{} {}", a.pcm.len(), b.pcm.len());
        let (la, lb) = (a.lufs.unwrap(), b.lufs.unwrap());
        assert!(la < -30.0 && (lb + 14.0).abs() < 1.5, "{la} → {lb}");
        let pic = b.pcm.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]).unsigned_abs()).max().unwrap() as f64 / 32768.0;
        assert!(pic <= 10f64.powf(-1.0 / 20.0), "{pic}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Un souffle de 3 s : DeepFilterNet en rend un WAV lisible, aussi long, bien plus calme ; un
    /// passage, lui, commence à son début.
    #[test]
    fn souffle_debruite() {
        let d = std::env::temp_dir().join(format!("debruite-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (v, wav) = (d.join("souffle.mp4"), d.join("souffle.wav"));
        let fait = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "anoisesrc=c=pink:r=44100:a=0.05:d=3:seed=42", "-ac", "2", "-c:a", "aac"])
            .arg(&v)
            .status()
            .unwrap();
        assert!(fait.success());
        let niveau = |f: &Path| {
            let o = Command::new("ffmpeg").args(["-hide_banner", "-i"]).arg(f).args(["-af", "volumedetect", "-f", "null", "-"]).output().unwrap();
            let t = String::from_utf8_lossy(&o.stderr).into_owned();
            let l = t.lines().find(|l| l.contains("mean_volume")).unwrap_or_else(|| panic!("{t}"));
            l.split(':').nth(1).unwrap().trim().trim_end_matches(" dB").parse::<f64>().unwrap()
        };
        let duree = |f: &Path| {
            let o = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=duration,sample_rate,channels", "-of", "csv=p=0"]).arg(f).output().unwrap();
            let t = String::from_utf8_lossy(&o.stdout).into_owned();
            let c: Vec<&str> = t.trim().split(',').collect();
            assert_eq!(&c[..2], ["48000", "1"], "{t}");
            c[2].parse::<f64>().unwrap()
        };
        debruiter_son(&v, None, 1.0, &wav, &Suivi::default()).unwrap();
        assert!((duree(&wav) - 3.0).abs() < 0.03, "{}", duree(&wav));
        let (avant, apres) = (niveau(&v), niveau(&wav));
        assert!(apres < avant - 20.0, "{avant} → {apres} dB");
        // À 6 dB au plus, le souffle baisse à peine.
        debruiter_son(&v, Some((1.0, 1.5)), 0.0, &wav, &Suivi::default()).unwrap();
        assert!((duree(&wav) - 1.5).abs() < 0.03, "{}", duree(&wav));
        let faible = niveau(&wav);
        assert!(faible > avant - 8.0, "{avant} → {faible} dB");
        // Arrêté en route : pas de WAV laissé.
        let arret = Suivi::default();
        arret.arret.store(true, Ordering::Relaxed);
        assert!(debruiter_son(&v, None, 1.0, &wav, &arret).is_err() && !wav.exists());
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Le rendu d'une prise bruitée, voix débruitée : la sortie est en stéréo, calme, et le WAV de
    /// passage a disparu.
    #[test]
    fn rendu_debruite() {
        let d = std::env::temp_dir().join(format!("rendu-debruite-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (v, s) = (d.join("prise.mp4"), d.join("sortie.mp4"));
        let fait = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=gray:s=64x48:r=30:d=3", "-f", "lavfi", "-i", "anoisesrc=c=pink:r=48000:a=0.05:d=3:seed=42"])
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&v)
            .status()
            .unwrap();
        assert!(fait.success());
        let m = Montage { duree: 3.0, largeur: 64, hauteur: 48, ..Montage::default() };
        let voix = Voix { bruit: true, bruit_force: 1.0, ..Voix::aucune() };
        rendre(&v, &m, &voix, &noyau::prereglages::PREREGLAGES[0], &s, &Suivi::default()).unwrap();
        let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=channels", "-of", "csv=p=0"]).arg(&s).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "2");
        let o = Command::new("ffmpeg").args(["-hide_banner", "-i"]).arg(&s).args(["-af", "volumedetect", "-f", "null", "-"]).output().unwrap();
        let t = String::from_utf8_lossy(&o.stderr).into_owned();
        let moyen: f64 = t.lines().find(|l| l.contains("mean_volume")).and_then(|l| l.split(':').nth(1)?.trim().trim_end_matches(" dB").parse().ok()).unwrap_or(-91.0);
        assert!(moyen < -45.0, "{moyen} dB");
        let restes: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(restes.len(), 2, "{restes:?}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn entete_wav_lisible() {
        let e = entete_wav(48_000);
        assert_eq!(&e[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([e[40], e[41], e[42], e[43]]), 192_000);
        assert_eq!(u32::from_le_bytes([e[4], e[5], e[6], e[7]]), 192_036);
        assert_eq!(chemin_voix_debruitee(Path::new("/a/b.partiel.mp4")), Path::new("/a/b.partiel.mp4.voix.wav"));
    }

    /// 76 coupes fondues : 152 facteurs. À la file, ffmpeg les refusait et le rendu échouait dès
    /// le départ (Christophe, 25/11/2026) ; en arbre, 400 coupes passent.
    #[test]
    fn longues_expressions_acceptees() {
        let fondus: Vec<(f64, f64, f64)> = (0..400).map(|i| (i as f64 * 0.01 + 0.002, i as f64 * 0.01 + 0.004, 0.002)).collect();
        let coupes: Vec<(f64, f64)> = fondus.iter().map(|&(a, b, _)| (a, b)).collect();
        let graphe = format!("{}aselect='not({})',asetpts=N/SR/TB", enveloppe_son(&fondus), expression_coupes(&coupes).unwrap());
        let o = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "sine=f=440:r=48000:d=5", "-af", &graphe, "-f", "null", "-"])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", cause_ffmpeg(&String::from_utf8_lossy(&o.stderr)));
        assert_eq!(en_arbre(&["a".into(), "b".into(), "c".into()], '+'), "(a+(b+c))");
    }

    #[test]
    fn cause_lisible() {
        let bavard = format!("[Parsed_volume_1 @ 0x5564] Error when evaluating the volume expression '{}'\n[AVFilterGraph @ 0x55] Error initializing filters\nError : Cannot allocate memory\n", "x*".repeat(3000));
        let c = cause_ffmpeg(&bavard);
        assert!(c.starts_with("Error when evaluating the volume expression") && c.ends_with("— Error : Cannot allocate memory") && c.chars().count() < 250, "{c}");
        assert_eq!(cause_ffmpeg(""), "ffmpeg s'est arrêté sans rien dire");
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
