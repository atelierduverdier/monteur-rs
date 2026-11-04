//! Les sous-titres : la transcription de `whisper-cli`, recollée en phrases lisibles, et le .srt à
//! part aux temps de la vidéo montée (`montage.py` du Monteur 1).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::arrondi::{entier, longueur};
use crate::montage::{Montage, SousTitre, temps_monte};

/// Un sous-titre plus court que ça (en s, ou en mots) se lit mal seul : il rejoint son voisin
/// s'il le suit de près, sans dépasser deux lignes.
pub const SOUS_TITRE_COURT_S: f64 = 1.3;
pub const SOUS_TITRE_COURT_MOTS: usize = 2;
pub const SOUS_TITRE_MAX: usize = 84;
/// Deux lignes de 42 caractères au plus.
pub const SRT_LIGNE: usize = 42;
pub const SRT_LIGNES: usize = 2;
/// Un sous-titre presque tout coupé ne s'écrit pas.
pub const SRT_COURT: f64 = 0.3;

/// Les modèles de la dictée, déjà sur ce poste.
pub fn modeles() -> PathBuf {
    std::env::home_dir().unwrap_or_default().join(".local/share/dictee/modeles")
}

pub fn commande_son_16k(video: &Path, wav: &Path) -> Vec<String> {
    ["ffmpeg", "-v", "error", "-y", "-i", &video.to_string_lossy(), "-vn", "-ac", "1", "-ar", "16000", &wav.to_string_lossy()]
        .map(String::from)
        .to_vec()
}

/// `whisper-cli` sur ce poste : segments de 60 caractères au plus, coupés entre deux mots, et la
/// détection de voix quand son modèle est là (sinon Whisper invente des phrases dans les
/// silences).
pub fn commande_whisper(wav: &Path, base: &Path, langue: &str, modeles: &Path) -> Vec<String> {
    let fils = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let mut cmd: Vec<String> = [
        "whisper-cli",
        "-m",
        &modeles.join("ggml-large-v3-turbo.bin").to_string_lossy(),
        "-l",
        langue,
        "-ml",
        "60",
        "-sow",
        "-oj",
        "-of",
        &base.to_string_lossy(),
        "-f",
        &wav.to_string_lossy(),
        "-t",
        &fils.to_string(),
    ]
    .map(String::from)
    .to_vec();
    let vad = modeles.join("ggml-silero-v5.1.2.bin");
    if vad.exists() {
        cmd.extend(["--vad".into(), "-vm".into(), vad.to_string_lossy().into_owned()]);
    }
    cmd
}

fn mots(s: &str) -> Vec<&str> {
    s.split_whitespace().collect()
}

/// Les sous-titres d'après le JSON de `whisper-cli` ; [Musique], [BLANK_AUDIO] sont écartés.
pub fn lire_whisper(json: &Value) -> Vec<SousTitre> {
    let mut res = Vec::new();
    for s in json.get("transcription").and_then(Value::as_array).into_iter().flatten() {
        let brut = match s.get("text") {
            Some(Value::String(t)) => t.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        };
        let texte = mots(&brut).join(" ");
        let o = s.get("offsets");
        let (Some(de), Some(a)) = (o.and_then(|o| o.get("from")), o.and_then(|o| o.get("to"))) else { continue };
        let (Some(de), Some(a)) = (de.as_f64(), a.as_f64()) else { continue };
        if texte.is_empty() || (texte.starts_with('[') && texte.ends_with(']')) {
            continue;
        }
        res.push(SousTitre { debut: de / 1000.0, fin: a / 1000.0, texte, autres: Default::default() });
    }
    fusionner_sous_titres(res)
}

fn court(s: &SousTitre) -> bool {
    s.fin - s.debut < SOUS_TITRE_COURT_S || mots(&s.texte).len() <= SOUS_TITRE_COURT_MOTS
}

/// « par symétrie », « finie. », « 10 mm. » seuls à l'écran une demi-seconde : collés à la phrase
/// dont ils sont la fin (ou, à défaut, au début de la suivante).
pub fn fusionner_sous_titres(sous_titres: Vec<SousTitre>) -> Vec<SousTitre> {
    let mut res: Vec<SousTitre> = Vec::new();
    for s in sous_titres {
        if let Some(p) = res.last_mut()
            && court(&s)
            && s.debut - p.fin < 0.5
            && longueur(&p.texte) + 1 + longueur(&s.texte) <= SOUS_TITRE_MAX
        {
            p.fin = s.fin;
            p.texte = format!("{} {}", p.texte, s.texte);
            continue;
        }
        res.push(SousTitre { autres: Default::default(), ..s });
    }
    // Un court resté seul en tête d'une suite : collé au début de la suivante.
    let mut i = 0;
    while i + 1 < res.len() {
        let (s, n) = (&res[i], &res[i + 1]);
        if court(s) && n.debut - s.fin < 0.5 && longueur(&s.texte) + 1 + longueur(&n.texte) <= SOUS_TITRE_MAX {
            let fusion = SousTitre { debut: s.debut, fin: n.fin, texte: format!("{} {}", s.texte, n.texte), autres: Default::default() };
            res.splice(i..i + 2, [fusion]);
        } else {
            i += 1;
        }
    }
    res
}

/// Les passages du son à retirer : les coupes, et la fin de chaque passage accéléré (son
/// silence n'en garde que la part que dure l'image accélérée).
pub fn son_retire(montage: &Montage) -> Vec<(f64, f64)> {
    let mut retire = montage.coupes_actives();
    retire.extend(montage.accelerations().iter().map(|&(a, b, v)| (a + (b - a) / v, b)));
    retire.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
    retire
}

/// L'image de l'instant `t` sort-elle ? Non dans une coupe ; une sur v dans un passage accéléré
/// v fois.
pub fn image_gardee(montage: &Montage, t: f64, ips: f64) -> bool {
    if montage.dans_une_coupe(t) {
        return false;
    }
    for (a, b, v) in montage.accelerations() {
        if a <= t && t < b {
            return entier((t - a) * ips).rem_euclid(entier(v).max(1)) == 0;
        }
    }
    true
}

/// Les mots en lignes de `SRT_LIGNE` caractères au plus (un mot plus long reste seul).
fn lignes_srt(mots: &[&str]) -> Vec<String> {
    let mut lignes: Vec<String> = Vec::new();
    for m in mots {
        match lignes.last_mut() {
            Some(l) if longueur(l) + 1 + longueur(m) <= SRT_LIGNE => {
                l.push(' ');
                l.push_str(m);
            }
            _ => lignes.push(m.to_string()),
        }
    }
    lignes
}

/// Un morceau en une ligne, ou en deux lignes à peu près égales.
fn deux_lignes(mots: &[&str]) -> Vec<String> {
    let texte = mots.join(" ");
    if longueur(&texte) <= SRT_LIGNE {
        return vec![texte];
    }
    let mut meilleur: Option<(usize, Vec<String>)> = None;
    for i in 1..mots.len() {
        let (a, b) = (mots[..i].join(" "), mots[i..].join(" "));
        let (la, lb) = (longueur(&a), longueur(&b));
        if la <= SRT_LIGNE && lb <= SRT_LIGNE {
            let ecart = la.abs_diff(lb);
            if meilleur.as_ref().is_none_or(|m| ecart < m.0) {
                meilleur = Some((ecart, vec![a, b]));
            }
        }
    }
    meilleur.map_or_else(|| lignes_srt(mots), |m| m.1)
}

/// Le texte en morceaux d'au plus deux lignes de 42 caractères, de longueurs voisines.
pub fn decouper_sous_titre(texte: &str) -> Vec<Vec<String>> {
    let mots = mots(texte);
    if mots.is_empty() {
        return Vec::new();
    }
    let mut n = lignes_srt(&mots).len().div_ceil(SRT_LIGNES).max(1);
    loop {
        // n morceaux d'à peu près la même longueur, coupés entre deux mots.
        let cible = (longueur(&mots.join(" ")) + 1) as f64 / n as f64;
        let mut morceaux: Vec<Vec<&str>> = Vec::new();
        let mut courant: Vec<&str> = Vec::new();
        let mut fait = 0usize;
        for m in &mots {
            // Le morceau se ferme quand ce mot le ferait finir plus loin de sa part que s'il
            // s'arrêtait avant lui.
            let borne = cible * (morceaux.len() + 1) as f64;
            if !courant.is_empty()
                && morceaux.len() < n - 1
                && (fait + longueur(m) + 1) as f64 - borne > borne - fait as f64
            {
                morceaux.push(std::mem::take(&mut courant));
            }
            courant.push(m);
            fait += longueur(m) + 1;
        }
        morceaux.push(courant);
        let rendus: Vec<Vec<String>> = morceaux.iter().map(|x| deux_lignes(x)).collect();
        if rendus.iter().all(|r| r.len() <= SRT_LIGNES) || n >= mots.len() {
            return rendus;
        }
        n += 1;
    }
}

fn horodatage(t: f64) -> String {
    let ms = entier(t.max(0.0) * 1000.0);
    let (h, ms) = (ms / 3_600_000, ms % 3_600_000);
    let (m, ms) = (ms / 60_000, ms % 60_000);
    let (s, ms) = (ms / 1000, ms % 1000);
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

/// Les sous-titres du montage en SubRip, aux temps de la vidéo montée (plus `decalage` : les 3 s
/// de vignette devant la vidéo envoyée). Un sous-titre trop long pour deux lignes se partage en
/// morceaux qui se partagent son temps à la longueur de leur texte.
pub fn texte_srt(montage: &Montage, decalage: f64) -> String {
    let retire = son_retire(montage);
    let mut tries: Vec<&SousTitre> = montage.sous_titres.iter().collect();
    tries.sort_by(|a, b| a.debut.total_cmp(&b.debut));
    let mut blocs: Vec<(f64, f64, Vec<String>)> = Vec::new();
    for st in tries {
        let (debut, fin) = (temps_monte(&retire, st.debut), temps_monte(&retire, st.fin));
        let morceaux = decouper_sous_titre(&st.texte);
        if fin - debut < SRT_COURT || morceaux.is_empty() {
            continue;
        }
        let longueurs: Vec<usize> = morceaux.iter().map(|m| m.iter().map(|l| longueur(l)).sum()).collect();
        let total: usize = longueurs.iter().sum();
        let mut fait = 0usize;
        for (m, n) in morceaux.into_iter().zip(longueurs) {
            let a = debut + (fin - debut) * fait as f64 / total as f64;
            fait += n;
            let b = debut + (fin - debut) * fait as f64 / total as f64;
            blocs.push((a + decalage, b + decalage, m));
        }
    }
    blocs
        .iter()
        .enumerate()
        .map(|(i, (a, b, lignes))| format!("{}\n{} --> {}\n{}\n\n", i + 1, horodatage(*a), horodatage(*b), lignes.join("\n")))
        .collect()
}

/// Écrit le .srt (sans jamais de moitié de fichier) ; rend le nombre de sous-titres écrits.
pub fn ecrire_srt(chemin: &Path, montage: &Montage, decalage: f64) -> std::io::Result<usize> {
    let texte = texte_srt(montage, decalage);
    crate::ecriture::ecrire_atomique(chemin, "srt", texte.as_bytes())?;
    Ok(texte.matches(" --> ").count())
}
