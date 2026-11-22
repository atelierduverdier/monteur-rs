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

/// Les modèles de la dictée, déjà sur le poste de Christophe.
pub fn modeles() -> PathBuf {
    crate::systeme::dossier_donnees().join("dictee/modeles")
}

/// Les modèles du Monteur lui-même : ceux qu'il télécharge.
pub fn modeles_du_monteur() -> PathBuf {
    crate::systeme::donnees().join("modeles")
}

/// Les modèles de Whisper, du meilleur au plus léger : le premier trouvé sert.
pub const MODELES: [&str; 5] = ["ggml-large-v3-turbo.bin", "ggml-large-v3-turbo-q5_0.bin", "ggml-medium.bin", "ggml-small.bin", "ggml-base.bin"];
/// Le modèle de détection de voix : sans lui, Whisper invente des phrases dans les silences.
pub const VAD: &str = "ggml-silero-v5.1.2.bin";

/// Un fichier à télécharger : son nom, d'où, sa taille et son empreinte SHA-256.
pub struct ATelecharger {
    pub nom: &'static str,
    pub url: &'static str,
    pub taille: u64,
    pub sha256: &'static str,
}

/// Ce que le Monteur télécharge pour les sous-titres : large-v3-turbo quantifié (le français y est presque aussi bon
/// que dans le modèle entier, pour le tiers de la place), et la détection de voix. Empreintes de huggingface.co,
/// relevées le 22/11/2026.
pub static A_TELECHARGER: [ATelecharger; 2] = [
    ATelecharger {
        nom: "ggml-large-v3-turbo-q5_0.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        taille: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    },
    ATelecharger {
        nom: VAD,
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        taille: 885_098,
        sha256: "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
    },
];

/// Le meilleur modèle présent : dans les modèles du Monteur, puis dans ceux de la dictée.
pub fn trouver_modele_dans(dossiers: &[PathBuf]) -> Option<PathBuf> {
    MODELES.iter().flat_map(|m| dossiers.iter().map(move |d| d.join(m))).find(|p| p.is_file())
}

pub fn trouver_modele() -> Option<PathBuf> {
    trouver_modele_dans(&[modeles_du_monteur(), modeles()])
}

/// Les langues proposées pour les sous-titres : le code de Whisper et son nom.
pub const LANGUES: [(&str, &str); 6] = [("fr", "Français"), ("en", "Anglais"), ("es", "Espagnol"), ("de", "Allemand"), ("it", "Italien"), ("auto", "Reconnue seule")];

fn chemin_langue() -> PathBuf {
    crate::systeme::donnees().join("langue-sous-titres")
}

/// La langue des sous-titres, le français d'office.
pub fn langue() -> String {
    std::fs::read_to_string(chemin_langue()).ok().map(|l| l.trim().to_string()).filter(|l| LANGUES.iter().any(|(c, _)| c == l)).unwrap_or_else(|| "fr".into())
}

pub fn ecrire_langue(code: &str) -> std::io::Result<()> {
    if let Some(d) = chemin_langue().parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(chemin_langue(), format!("{code}\n"))
}

pub fn commande_son_16k(video: &Path, wav: &Path) -> Vec<String> {
    ["ffmpeg", "-v", "error", "-y", "-i", &video.to_string_lossy(), "-vn", "-ac", "1", "-ar", "16000", &wav.to_string_lossy()]
        .map(String::from)
        .to_vec()
}

/// `whisper-cli` : segments de 60 caractères au plus, coupés entre deux mots, et la détection de
/// voix quand son modèle est à côté de celui de Whisper (sinon Whisper invente des phrases dans
/// les silences).
pub fn commande_whisper(wav: &Path, base: &Path, langue: &str, modele: &Path) -> Vec<String> {
    let fils = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    let mut cmd: Vec<String> = [
        "whisper-cli",
        "-m",
        &modele.to_string_lossy(),
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
    let vad = modele.with_file_name(VAD);
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
        // Une vidéo intercalée avant lui le repousse d'autant.
        let ajoute: f64 = montage.insertions.iter().filter(|i| !i.par_dessus() && i.t <= st.debut).map(|i| i.duree()).sum();
        let (debut, fin) = (temps_monte(&retire, st.debut) + ajoute, temps_monte(&retire, st.fin) + ajoute);
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

/// Coupe un texte en deux à peu près à la part `part` (0..1) de ses mots ; `None` s'il n'a qu'un mot.
pub fn couper_texte(texte: &str, part: f64) -> Option<(String, String)> {
    let m = mots(texte);
    if m.len() < 2 {
        return None;
    }
    let i = ((part * m.len() as f64).round() as usize).clamp(1, m.len() - 1);
    Some((m[..i].join(" "), m[i..].join(" ")))
}

/// L'instant `t` de la prise dans la vidéo montée : coupes et accélérés retirés, vidéos intercalées
/// avant lui ajoutées.
pub fn temps_dans_le_montage(m: &Montage, t: f64) -> f64 {
    let ajoute: f64 = m.insertions.iter().filter(|i| !i.par_dessus() && i.t <= t).map(|i| i.duree()).sum();
    temps_monte(&son_retire(m), t) + ajoute
}

/// « 1:02 », « 1:02:03 » : comme YouTube les lit dans une description.
pub fn temps_youtube(t: f64) -> String {
    let s = t.max(0.0).floor() as i64;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Les chapitres pour une description YouTube, aux temps de la vidéo envoyée (`decalage` : les 3 s
/// de vignette). Le premier est toujours à 0:00, sinon YouTube ne les reconnaît pas.
pub fn texte_chapitres(m: &Montage, decalage: f64) -> String {
    let mut lignes = Vec::new();
    for (i, c) in m.chapitres.iter().enumerate() {
        let t = if i == 0 { 0.0 } else { temps_dans_le_montage(m, c.t) + decalage };
        lignes.push(format!("{} {}", temps_youtube(t), c.titre.trim()));
    }
    lignes.join("\n")
}

/// Ce qui empêcherait YouTube de reconnaître les chapitres : 3 au moins, 10 s chacun au moins.
pub fn problemes_chapitres(m: &Montage) -> Vec<String> {
    let mut p = Vec::new();
    if m.chapitres.is_empty() {
        return p;
    }
    if m.chapitres.len() < 3 {
        p.push(format!("il faut 3 chapitres au moins pour YouTube ({} posé(s))", m.chapitres.len()));
    }
    let t: Vec<f64> = m.chapitres.iter().map(|c| temps_dans_le_montage(m, c.t)).collect();
    if t.windows(2).any(|w| w[1] - w[0] < 10.0) {
        p.push("deux chapitres à moins de 10 s l'un de l'autre : YouTube les refuserait".into());
    }
    p
}

/// La description où remplacer les chapitres : les anciennes lignes « 1:02 Titre » retirées, le
/// bloc neuf à la fin du texte.
pub fn inserer_chapitres(texte: &str, chapitres: &str) -> String {
    let est_chapitre = |l: &str| {
        let t = l.split_whitespace().next().unwrap_or("");
        t.contains(':') && t.chars().all(|c| c.is_ascii_digit() || c == ':') && l.len() > t.len()
    };
    let reste: Vec<&str> = texte.lines().filter(|l| !est_chapitre(l)).collect();
    let reste = reste.join("\n");
    let reste = reste.trim();
    if reste.is_empty() { chapitres.to_string() } else { format!("{reste}\n\n{chapitres}") }
}

#[cfg(test)]
mod tests_modeles {
    use super::*;

    #[test]
    fn le_meilleur_modele_present() {
        let d = std::env::temp_dir().join(format!("monteur-modeles-{}", std::process::id()));
        let (monteur, dictee) = (d.join("monteur"), d.join("dictee"));
        std::fs::create_dir_all(&monteur).unwrap();
        std::fs::create_dir_all(&dictee).unwrap();
        let dossiers = [monteur.clone(), dictee.clone()];
        assert_eq!(trouver_modele_dans(&dossiers), None);
        std::fs::write(monteur.join("ggml-small.bin"), "").unwrap();
        assert_eq!(trouver_modele_dans(&dossiers), Some(monteur.join("ggml-small.bin")));
        // Le modèle entier de la dictée passe devant un plus léger du Monteur.
        std::fs::write(dictee.join("ggml-large-v3-turbo.bin"), "").unwrap();
        assert_eq!(trouver_modele_dans(&dossiers), Some(dictee.join("ggml-large-v3-turbo.bin")));
        // La détection de voix se prend à côté du modèle choisi.
        std::fs::write(dictee.join(VAD), "").unwrap();
        let c = commande_whisper(Path::new("s.wav"), Path::new("b"), "en", &dictee.join("ggml-large-v3-turbo.bin")).join(" ");
        let _ = std::fs::remove_dir_all(&d);
        assert!(c.contains("-l en") && c.contains("--vad -vm") && c.contains(VAD), "{c}");
    }

    #[test]
    fn la_liste_a_telecharger_tient() {
        for f in &A_TELECHARGER {
            assert_eq!(f.sha256.len(), 64, "{}", f.nom);
            assert!(f.url.ends_with(f.nom) && f.url.starts_with("https://huggingface.co/"), "{}", f.url);
        }
        assert!(MODELES.contains(&A_TELECHARGER[0].nom), "le modèle téléchargé est cherché");
    }
}
