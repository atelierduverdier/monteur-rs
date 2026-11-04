//! Les temps morts : un silence ET rien à faire à l'écran. Un silence où l'image bouge d'elle-même
//! (une simulation qui tourne) s'accélère au lieu de se couper (`montage.py` du Monteur 1).

use std::path::Path;

use crate::analyse::mouvement_moyen;
use crate::arrondi::arrondi;
use crate::montage::Coupe;

pub const SILENCE_DB: i32 = -35;
/// Un silence plus court n'est pas un temps mort.
pub const SILENCE_MIN: f64 = 1.2;
/// On laisse ce souffle de part et d'autre d'une coupe.
pub const GARDE: f64 = 0.35;
pub const ACCELERER_MIN: f64 = 3.0;
pub const IMAGE_BOUGE: f64 = 0.9;
pub const ACCELERATION: f64 = 4.0;

pub fn commande_silences(video: &Path) -> Vec<String> {
    [
        "ffmpeg",
        "-hide_banner",
        "-nostats",
        "-i",
        &video.to_string_lossy(),
        "-vn",
        "-af",
        &format!("silencedetect=noise={SILENCE_DB}dB:d={SILENCE_MIN}"),
        "-f",
        "null",
        "-",
    ]
    .map(String::from)
    .to_vec()
}

/// Le nombre écrit après `cle` dans la ligne (`-?[\d.]+` du Monteur 1).
fn nombre_apres(ligne: &str, cle: &str, signe: bool) -> Option<f64> {
    let reste = &ligne[ligne.find(cle)? + cle.len()..];
    let mut fin = 0;
    for (i, c) in reste.char_indices() {
        if c.is_ascii_digit() || c == '.' || (signe && i == 0 && c == '-') {
            fin = i + c.len_utf8();
        } else {
            break;
        }
    }
    reste[..fin].parse().ok()
}

/// (début, fin) d'après la sortie de `silencedetect` ; un silence qui court jusqu'au bout finit à
/// la durée.
pub fn lire_silences(texte: &str, duree: f64) -> Vec<(f64, f64)> {
    let mut silences = Vec::new();
    let mut debut: Option<f64> = None;
    for ligne in texte.lines() {
        if let Some(d) = nombre_apres(ligne, "silence_start: ", true) {
            debut = Some(d.max(0.0));
        }
        if let Some(f) = nombre_apres(ligne, "silence_end: ", false)
            && let Some(d) = debut.take()
        {
            silences.push((d, f));
        }
    }
    if let Some(d) = debut {
        silences.push((d, duree));
    }
    silences
}

/// Les passages proposés : les silences, moins les moments où l'on agit à l'écran (un réglage
/// qu'on tape en silence se regarde), moins une garde. `actifs` : les instants des gestes (`None`
/// sans journal : tout est seulement proposé, rien n'est coupé).
pub fn temps_morts(silences: &[(f64, f64)], actifs: Option<&[f64]>, mouvement: Option<&[f64]>) -> Vec<Coupe> {
    let avec_journal = actifs.is_some();
    let mut actifs: Vec<f64> = actifs.unwrap_or_default().to_vec();
    actifs.sort_by(f64::total_cmp);
    let mut coupes = Vec::new();
    for &(a, b) in silences {
        let mut morceaux = vec![(a, b)];
        if !actifs.is_empty() {
            morceaux.clear();
            let mut debut = a;
            for &t in actifs.iter().filter(|&&t| a <= t && t <= b) {
                if t - debut >= SILENCE_MIN {
                    morceaux.push((debut, t - 0.5));
                }
                debut = debut.max(t + 0.8); // on laisse voir la fin d'un geste
            }
            if b - debut >= SILENCE_MIN {
                morceaux.push((debut, b));
            }
        }
        for (x, y) in morceaux {
            let (x, y) = (x + GARDE, y - GARDE);
            if y - x < SILENCE_MIN - 2.0 * GARDE {
                continue;
            }
            let bouge = mouvement
                .filter(|m| !m.is_empty())
                .is_some_and(|m| y - x >= ACCELERER_MIN && mouvement_moyen(m, x, y) > IMAGE_BOUGE);
            coupes.push(Coupe {
                debut: arrondi(x, 3),
                fin: arrondi(y, 3),
                active: avec_journal,
                origine: "temps mort".into(),
                vitesse: if bouge { ACCELERATION } else { 0.0 },
                autres: Default::default(),
            });
        }
    }
    coupes
}
