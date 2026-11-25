//! Les vidéos d'un dossier et leur état, le rangement, ce que ffprobe en dit, le cadre de la
//! vignette, le montage « 3 s de vignette puis la vidéo » et l'envoi par `youtube-envoi`
//! (`noyau.py` du Monteur 1). La vignette elle-même ne passe plus par ImageMagick : elle se
//! peindra avec le reste (étape 2).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::arrondi::{entier, longueur};
use crate::description::{Description, normaliser_mots_cles};
use crate::fiches::{Fiche, Fiches};
use crate::{
    DUREE_CARTON, DUREE_VIGNETTE, EXTENSIONS, Erreur, FOND_MONTAGE, IMAGE_H, IMAGE_L, SUFFIXE_MONTEE, SUFFIXE_PNG, SUFFIXES_DERIVES,
    SUFFIXES_LIES,
};

pub fn dossier_defaut() -> PathBuf {
    crate::systeme::videos().join("Copies d'écran")
}

pub fn rangement_defaut() -> PathBuf {
    crate::systeme::videos().join("YouTube")
}

/// `Path.stem` de Python : le nom sans sa dernière extension.
fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn extension(p: &Path) -> String {
    p.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Le nom (sans extension) de la vidéo d'origine : « X » pour « X avec vignette.mp4 » comme
/// pour « X.mp4 ».
pub fn nom_source(video: &Path) -> String {
    let nom = stem(video);
    for suffixe in SUFFIXES_DERIVES {
        if let Some(source) = nom.strip_suffix(suffixe) {
            return source.to_string();
        }
    }
    nom
}

/// Une vidéo montée dont l'original n'est plus là : elle se lit, s'envoie, mais ne se remonte pas.
pub fn est_derivee(video: &Path) -> bool {
    stem(video) != nom_source(video)
}

/// « <dossier>/<nom de la vidéo d'origine><suffixe> ».
pub fn chemin_derive(video: &Path, suffixe: &str) -> PathBuf {
    video.with_file_name(format!("{}{suffixe}", nom_source(video)))
}

/// Un rendu s'écrit sous ce nom, renommé une fois fini : une vidéo à moitié faite ne passe
/// jamais pour finie.
pub fn chemin_partiel(sortie: &Path) -> PathBuf {
    let ext = sortie.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    sortie.with_file_name(format!("{}.partiel{ext}", stem(sortie)))
}

fn date(p: &Path) -> SystemTime {
    std::fs::metadata(p).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Les vidéos sources du dossier, la plus récente d'abord, et les vidéos montées dont l'original
/// n'est plus là.
pub fn lister_videos(dossier: &Path) -> Vec<PathBuf> {
    let Ok(entrees) = std::fs::read_dir(dossier) else { return Vec::new() };
    let videos: Vec<PathBuf> = entrees
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && EXTENSIONS.contains(&extension(p).as_str()) && !stem(p).ends_with(".partiel"))
        // La webcam filmée avec une prise n'en est pas une.
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(crate::SUFFIXE_WEBCAM)))
        .collect();
    let mut sources: Vec<&PathBuf> = videos.iter().filter(|p| !est_derivee(p)).collect();
    // Deux sources de même nom (X.mkv d'OBS et X.mp4 remuxé) partageraient vignette, description
    // et montage : on n'en garde qu'une, le .mp4.
    sources.sort_by(|a, b| (extension(a) != "mp4").cmp(&(extension(b) != "mp4")).then(date(b).cmp(&date(a))));
    let mut par_nom: Vec<(String, PathBuf)> = Vec::new();
    let mut garder = |nom: String, p: &PathBuf| {
        if !par_nom.iter().any(|(n, _)| *n == nom) {
            par_nom.push((nom, p.clone()));
        }
    };
    for p in sources {
        garder(stem(p), p);
    }
    for p in &videos {
        if p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(SUFFIXE_MONTEE)) {
            garder(nom_source(p), p);
        }
    }
    let mut liste: Vec<PathBuf> = par_nom.into_iter().map(|(_, p)| p).collect();
    liste.sort_by_key(|p| std::cmp::Reverse(date(p)));
    liste
}

/// La vidéo et ce qui l'accompagne sur le disque : ce qui part avec elle quand on la range.
pub fn fichiers_lies(video: &Path) -> Vec<PathBuf> {
    let mut fichiers = vec![video.to_path_buf()];
    for suffixe in SUFFIXES_LIES {
        let f = chemin_derive(video, suffixe);
        if f.exists() && !fichiers.contains(&f) {
            fichiers.push(f);
        }
    }
    fichiers
}

/// Où en est la vidéo : brute, vignette, montée, envoyée (ou ignorée).
pub fn etat(video: &Path, fiche: Option<&Fiche>) -> &'static str {
    if fiche.is_some_and(|f| f.ignoree) {
        return "ignorée";
    }
    if fiche.is_some_and(|f| !f.youtube_id.is_empty()) {
        return "envoyée";
    }
    if chemin_derive(video, SUFFIXE_MONTEE).exists() {
        return "montée";
    }
    if chemin_derive(video, SUFFIXE_PNG).exists() {
        return "vignette";
    }
    "brute"
}

/// La clé d'une vidéo dans les fiches : toujours son chemin absolu.
pub fn cle(video: &Path) -> String {
    let absolu = std::path::absolute(video).unwrap_or_else(|_| video.to_path_buf());
    absolu.canonicalize().unwrap_or(absolu).to_string_lossy().into_owned()
}

fn deplacer_fichier(source: &Path, cible: &Path) -> std::io::Result<()> {
    if std::fs::rename(source, cible).is_ok() {
        return Ok(());
    }
    // Pas le même disque : copie sous un nom partiel, renommage, puis seulement l'original
    // effacé. Une coupure ne laisse jamais une copie tronquée sous le vrai nom.
    let partiel = chemin_partiel(cible);
    std::fs::copy(source, &partiel)?;
    std::fs::rename(&partiel, cible)?;
    std::fs::remove_file(source)
}

/// Déplace la vidéo, ce qui l'accompagne et sa fiche dans `destination`. Rien n'est écrasé : si
/// un des fichiers existe déjà là-bas, rien ne bouge. La vidéo part la première et sa fiche la suit
/// aussitôt (écrite dans `chemin_fiches`) ; si un fichier lié refuse ensuite, l'erreur le nomme.
pub fn deplacer(fiches: &mut Fiches, chemin_fiches: &Path, video: &Path, destination: &Path) -> Result<PathBuf, Erreur> {
    let video = PathBuf::from(cle(video));
    std::fs::create_dir_all(destination).map_err(|e| Erreur(format!("{} : {e}", destination.display())))?;
    let destination = PathBuf::from(cle(destination));
    if video.parent() == Some(destination.as_path()) {
        return Ok(video);
    }
    let fichiers = fichiers_lies(&video);
    let deja: Vec<String> = fichiers
        .iter()
        .filter_map(|f| f.file_name())
        .filter(|n| destination.join(n).exists())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    if !deja.is_empty() {
        return Err(Erreur(format!("déjà dans {} : {}", destination.display(), deja.join(", "))));
    }
    let nom = video.file_name().expect("une vidéo a un nom");
    let nouvelle = destination.join(nom);
    deplacer_fichier(&video, &nouvelle).map_err(|e| Erreur(format!("{} : {e}", video.display())))?;
    if let Some(fiche) = fiches.fiches.remove(&video.to_string_lossy().into_owned()) {
        fiches.fiches.insert(cle(&nouvelle), fiche);
        fiches
            .ecrire(chemin_fiches, &dossier_defaut(), &rangement_defaut())
            .map_err(|e| Erreur(format!("fiches : {e}")))?;
    }
    let restes: Vec<String> = fichiers[1..]
        .iter()
        .filter_map(|f| {
            let n = f.file_name()?;
            deplacer_fichier(f, &destination.join(n)).err().map(|e| format!("{} ({e})", n.to_string_lossy()))
        })
        .collect();
    if !restes.is_empty() {
        return Err(Erreur(format!(
            "{} est dans {}, mais pas : {}",
            nom.to_string_lossy(),
            destination.display(),
            restes.join(", ")
        )));
    }
    Ok(nouvelle)
}

// ---------------------------------------------------------------------------
//  Ce que ffprobe dit d'une vidéo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Infos {
    pub duree: f64,
    pub largeur: i64,
    pub hauteur: i64,
    /// Tel que ffprobe le donne, ex. « 30/1 ».
    pub images_par_s: String,
    pub son: bool,
    pub frequence: i64,
    pub disposition: String,
}

pub fn commande_sonder(video: &Path) -> Vec<String> {
    ["ffprobe", "-v", "error", "-print_format", "json", "-show_format", "-show_streams", &video.to_string_lossy()]
        .map(String::from)
        .to_vec()
}

fn nombre(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) if s.is_empty() => None,
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// `a or b` de Python sur des valeurs JSON : la première qui n'est pas « fausse » (absente,
/// nulle, 0, chaîne vide).
fn ou<'a>(a: Option<&'a Value>, b: Option<&'a Value>) -> Option<&'a Value> {
    let faux = |v: &Value| match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        _ => false,
    };
    a.filter(|v| !faux(v)).or(b.filter(|v| !faux(v)))
}

/// Les infos d'après la sortie de `commande_sonder`.
pub fn lire_infos(sortie: &str) -> Result<Infos, Erreur> {
    let d: Value = serde_json::from_str(sortie).map_err(|e| Erreur(format!("ffprobe illisible ({e})")))?;
    let flux = d.get("streams").and_then(Value::as_array).cloned().unwrap_or_default();
    let de_sorte = |sorte: &str| flux.iter().find(|s| s.get("codec_type").and_then(Value::as_str) == Some(sorte)).cloned();
    let image = de_sorte("video").ok_or_else(|| Erreur("pas d'image dans cette vidéo".into()))?;
    let son = de_sorte("audio");
    let duree = nombre(ou(d.get("format").and_then(|f| f.get("duration")), image.get("duration"))).unwrap_or(0.0);
    let taille = |nom: &str| nombre(image.get(nom)).map(|x| x as i64).ok_or_else(|| Erreur(format!("ffprobe : pas de {nom}")));
    let mut infos = Infos {
        duree,
        largeur: taille("width")?,
        hauteur: taille("height")?,
        images_par_s: image
            .get("r_frame_rate")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("30/1")
            .to_string(),
        son: son.is_some(),
        frequence: 48000,
        disposition: "stereo".into(),
    };
    if let Some(s) = son {
        infos.frequence = nombre(ou(s.get("sample_rate"), None)).map_or(48000, |x| x as i64);
        let canaux = nombre(ou(s.get("channels"), None)).map_or(2, |x| x as i64);
        infos.disposition = match s.get("channel_layout").and_then(Value::as_str).filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None if canaux == 1 => "mono".into(),
            None => "stereo".into(),
        };
    }
    Ok(infos)
}

// ---------------------------------------------------------------------------
//  Le cadre : la part de l'image qui va à droite de la vignette
// ---------------------------------------------------------------------------

pub const LARGEUR_CADRE_MIN: i64 = 160;

/// Le plus grand cadre aux proportions 646:720, centré.
pub fn cadre_defaut(largeur: i64, hauteur: i64) -> [i64; 4] {
    let mut h = hauteur;
    let mut l = entier(h as f64 * IMAGE_L as f64 / IMAGE_H as f64);
    if l > largeur {
        l = largeur;
        h = entier(l as f64 * IMAGE_H as f64 / IMAGE_L as f64);
    }
    [(largeur - l).div_euclid(2), (hauteur - h).div_euclid(2), l, h]
}

/// Ramène le cadre dans l'image, aux bonnes proportions.
pub fn borner_cadre(cadre: [f64; 4], largeur: i64, hauteur: i64) -> [i64; 4] {
    let [x, y, l, _] = cadre;
    let lmax = largeur.min(entier(hauteur as f64 * IMAGE_L as f64 / IMAGE_H as f64));
    let l = LARGEUR_CADRE_MIN.min(lmax).max(entier(l).min(lmax));
    // L'arrondi de lmax peut donner un pixel de trop en hauteur (900 de haut : l = 808,
    // h = 901) : le cadre doit rester dans l'image.
    let h = entier(l as f64 * IMAGE_H as f64 / IMAGE_L as f64).min(hauteur);
    let x = entier(x).max(0).min(largeur - l);
    let y = entier(y).max(0).min(hauteur - h);
    [x, y, l, h]
}

/// ffmpeg qui sort en PNG, sur sa sortie standard, l'image à `temps`.
pub fn commande_image(video: &Path, temps: f64, duree: Option<f64>) -> Vec<String> {
    let mut temps = temps;
    if let Some(d) = duree.filter(|&d| d != 0.0) {
        // Au-delà de la dernière image, ffmpeg ne sort rien du tout.
        temps = temps.min((d - 0.1).max(0.0));
    }
    let ss = format!("{:.3}", temps.max(0.0));
    ["ffmpeg", "-v", "error", "-ss", &ss, "-i", &video.to_string_lossy(), "-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"]
        .map(String::from)
        .to_vec()
}

// ---------------------------------------------------------------------------
//  Le montage pour YouTube : 3 s de vignette, puis la vidéo
// ---------------------------------------------------------------------------

/// ffmpeg qui colle 3 s de vignette (silencieuses) devant la vidéo. La vignette est agrandie à la
/// largeur de la vidéo, posée sur un fond sombre à sa taille. Avancement sur la sortie standard
/// (-progress), en microsecondes.
/// La vignette 3 s en tête de la vidéo, et le carton de fin s'il y en a un (`DUREE_CARTON`, en queue). Sans carton,
/// la commande est celle du Monteur 1.
pub fn commande_montage(video: &Path, vignette: &Path, sortie: &Path, infos: &Infos, carton: Option<&Path>) -> Vec<String> {
    // libx264 en 4:2:0 veut des dimensions paires : la dernière colonne et la dernière ligne
    // d'une capture impaire sont rognées, pas rééchantillonnées.
    let l = infos.largeur - infos.largeur % 2;
    let h = infos.hauteur - infos.hauteur % 2;
    let ips = &infos.images_par_s;
    let s = |x: &str| x.to_string();
    // -f image2 -pattern_type none : sans eux, un « %d » dans le nom de la vignette serait lu
    // comme une suite d'images numérotées.
    let mut cmd: Vec<String> = vec![
        s("ffmpeg"), s("-v"), s("error"), s("-y"), s("-nostats"), s("-progress"), s("pipe:1"), s("-loop"), s("1"),
        s("-framerate"), ips.clone(), s("-t"), format!("{DUREE_VIGNETTE:?}"), s("-f"), s("image2"),
        s("-pattern_type"), s("none"), s("-i"), vignette.to_string_lossy().into_owned(),
    ];
    let pair = format!("crop={l}:{h}:0:0,setsar=1,format=yuv420p");
    let tete = format!(
        "[0:v]scale={l}:{h}:force_original_aspect_ratio=decrease:flags=lanczos,\
         pad={l}:{h}:(ow-iw)/2:(oh-ih)/2:color={FOND_MONTAGE},format=yuv420p,setsar=1,fps={ips}[v0]"
    );
    // Le carton : une image tenue DUREE_CARTON, mise à la taille comme la vignette, et du silence s'il y a du son.
    let image_tenue = |chemin: &Path, duree: f64| {
        vec![s("-loop"), s("1"), s("-framerate"), ips.clone(), s("-t"), format!("{duree:?}"), s("-f"), s("image2"), s("-pattern_type"), s("none"), s("-i"), chemin.to_string_lossy().into_owned()]
    };
    let silence = |duree: f64| vec![s("-f"), s("lavfi"), s("-t"), format!("{duree:?}"), s("-i"), format!("anullsrc=r={}:cl={}", infos.frequence, infos.disposition)];
    let a_la_taille = |entree: usize, sortie: &str| {
        format!(
            "[{entree}:v]scale={l}:{h}:force_original_aspect_ratio=decrease:flags=lanczos,\
             pad={l}:{h}:(ow-iw)/2:(oh-ih)/2:color={FOND_MONTAGE},format=yuv420p,setsar=1,fps={ips}[{sortie}]"
        )
    };
    let (filtre, sorties): (String, Vec<String>) = match (infos.son, carton) {
        (true, None) => {
            cmd.extend(silence(DUREE_VIGNETTE));
            cmd.extend([s("-i"), video.to_string_lossy().into_owned()]);
            (
                format!("{tete};[2:v]{pair}[v1];[v0][1:a][v1][2:a]concat=n=2:v=1:a=1[v][a]"),
                vec![s("-map"), s("[v]"), s("-map"), s("[a]"), s("-c:a"), s("aac"), s("-b:a"), s("192k")],
            )
        }
        (false, None) => {
            cmd.extend([s("-i"), video.to_string_lossy().into_owned()]);
            (format!("{tete};[1:v]{pair}[v1];[v0][v1]concat=n=2:v=1:a=0[v]"), vec![s("-map"), s("[v]")])
        }
        (true, Some(c)) => {
            cmd.extend(silence(DUREE_VIGNETTE));
            cmd.extend([s("-i"), video.to_string_lossy().into_owned()]);
            cmd.extend(image_tenue(c, DUREE_CARTON));
            cmd.extend(silence(DUREE_CARTON));
            (
                format!("{tete};[2:v]{pair}[v1];{};[v0][1:a][v1][2:a][v2][4:a]concat=n=3:v=1:a=1[v][a]", a_la_taille(3, "v2")),
                vec![s("-map"), s("[v]"), s("-map"), s("[a]"), s("-c:a"), s("aac"), s("-b:a"), s("192k")],
            )
        }
        (false, Some(c)) => {
            cmd.extend([s("-i"), video.to_string_lossy().into_owned()]);
            cmd.extend(image_tenue(c, DUREE_CARTON));
            (format!("{tete};[1:v]{pair}[v1];{};[v0][v1][v2]concat=n=3:v=1:a=0[v]", a_la_taille(2, "v2")), vec![s("-map"), s("[v]")])
        }
    };
    cmd.extend([s("-filter_complex"), filtre]);
    cmd.extend(sorties);
    cmd.extend(
        ["-c:v", "libx264", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p", "-movflags", "+faststart"].map(String::from),
    );
    cmd.push(sortie.to_string_lossy().into_owned());
    cmd
}

/// Fraction faite (0..1) d'après une ligne de -progress, sinon `None`.
pub fn avancement_montage(ligne: &str, infos: &Infos) -> Option<f64> {
    let us: u64 = ligne.trim().strip_prefix("out_time_us=")?.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
    let total = DUREE_VIGNETTE + infos.duree;
    (total != 0.0).then(|| (us as f64 / 1e6 / total).min(1.0))
}

// ---------------------------------------------------------------------------
//  YouTube
// ---------------------------------------------------------------------------

/// Limites de YouTube : au-delà, l'envoi est refusé.
pub const TITRE_MAX: usize = 100;
/// En octets.
pub const DESCRIPTION_MAX: usize = 5000;
pub const MOTS_CLES_MAX: usize = 500;

/// Ce que YouTube refuserait, en clair. Liste vide : tout va bien.
pub fn problemes_description(d: &Description) -> Vec<String> {
    let mut p = Vec::new();
    if d.titre.trim().is_empty() {
        p.push("le titre YouTube est vide".to_string());
    }
    if longueur(&d.titre) > TITRE_MAX {
        p.push(format!("titre trop long : {} caractères, {TITRE_MAX} au plus", longueur(&d.titre)));
    }
    if d.texte.len() > DESCRIPTION_MAX {
        p.push(format!("description trop longue : {} octets, {DESCRIPTION_MAX} au plus", d.texte.len()));
    }
    if longueur(&normaliser_mots_cles(&d.mots_cles)) > MOTS_CLES_MAX {
        p.push(format!("mots-clés trop longs : {MOTS_CLES_MAX} caractères au plus en tout"));
    }
    for (champ, valeur) in [("titre", &d.titre), ("description", &d.texte)] {
        if valeur.contains('<') || valeur.contains('>') {
            p.push(format!("YouTube refuse les signes < et > dans le {champ}"));
        }
    }
    p
}

/// `youtube-envoi`, ou le programme désigné par `MONTEUR_YOUTUBE_ENVOI` : les tests y mettent un
/// faux, pour ne jamais rien envoyer.
pub fn programme_envoi() -> String {
    if let Ok(force) = std::env::var("MONTEUR_YOUTUBE_ENVOI")
        && !force.is_empty()
    {
        return force;
    }
    crate::systeme::trouver_programme("youtube-envoi")
        .unwrap_or_else(|| crate::systeme::maison().join(".local/bin/youtube-envoi"))
        .to_string_lossy()
        .into_owned()
}

/// Envoi en privé, toujours : la mise en public reste un geste fait dans YouTube Studio.
pub fn commande_envoi(video: &Path, d: &Description, fichier_description: &Path, vignette: Option<&Path>) -> Vec<String> {
    let mut cmd = vec![
        programme_envoi(),
        "envoyer".into(),
        video.to_string_lossy().into_owned(),
        "--titre".into(),
        d.titre.trim().to_string(),
        "--description-fichier".into(),
        fichier_description.to_string_lossy().into_owned(),
        "--confidentialite".into(),
        "privee".into(),
    ];
    if let Some(v) = vignette {
        cmd.extend(["--vignette".into(), v.to_string_lossy().into_owned()]);
    }
    let mots = normaliser_mots_cles(&d.mots_cles);
    if !mots.is_empty() {
        cmd.extend(["--tags".into(), mots]);
    }
    cmd
}

/// L'identifiant de la vidéo envoyée, d'après la sortie de `youtube-envoi`.
pub fn lire_id_youtube(sortie: &str) -> Option<String> {
    let debut = sortie.find("https://youtu.be/")? + "https://youtu.be/".len();
    let id: String = sortie[debut..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
    (id.chars().count() >= 6).then_some(id)
}

/// Le pourcentage d'une ligne d'avancement (« 42,5 % »).
pub fn lire_pourcentage(ligne: &str) -> Option<f64> {
    let c: Vec<char> = ligne.chars().collect();
    for i in 0..c.len() {
        if !c[i].is_ascii_digit() || (i > 0 && (c[i - 1].is_ascii_digit())) {
            continue;
        }
        let mut j = i;
        while j < c.len() && c[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < c.len() && (c[j] == '.' || c[j] == ',') && c[j + 1].is_ascii_digit() {
            j += 1;
            while j < c.len() && c[j].is_ascii_digit() {
                j += 1;
            }
        }
        let mut k = j;
        while k < c.len() && c[k].is_whitespace() {
            k += 1;
        }
        if k < c.len() && c[k] == '%' {
            return c[i..j].iter().collect::<String>().replace(',', ".").parse().ok();
        }
    }
    None
}

/// Le fichier `.ts` de gpu-screen-recorder : l'instant de la première image, en secondes.
pub fn lire_premiere_image(texte: &str) -> Option<f64> {
    let valeurs = texte.lines().nth(1)?;
    Some(valeurs.split_whitespace().nth(1)?.parse::<f64>().ok()? / 1e6)
}

/// Le nom d'une prise à cet instant, à la façon d'OBS : `2026-11-05 07-47-00`.
pub fn nom_de_prise(instant: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(instant as i64, 0).earliest().map_or_else(String::new, |d| d.format("%Y-%m-%d %H-%M-%S").to_string())
}

#[cfg(test)]
mod tests_capture {
    use super::*;

    /// Le nom donné à une prise redonne son instant, à la seconde : les gestes se calent dessus.
    #[test]
    fn nom_de_prise_et_journal() {
        let t = lire_premiere_image("monotonic_microsec\trealtime_microsec\n5552684605\t1793861451459387\n").unwrap();
        assert!((t - 1793861451.459387).abs() < 1e-6);
        let v = std::path::PathBuf::from(format!("{}.mp4", nom_de_prise(t)));
        assert_eq!(crate::journal::debut_de_la_video(&v, 10.0), 1793861451.0);
        assert_eq!(lire_premiere_image("monotonic_microsec\trealtime_microsec\n"), None);
    }
}
