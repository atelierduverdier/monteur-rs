//! Écrire un fichier sans jamais laisser de moitié de fichier : un temporaire dans le même
//! dossier, puis un renommage. Une coupure de courant laisse l'ancien fichier entier.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::Serialize;

static COMPTEUR: AtomicU32 = AtomicU32::new(0);

/// Le JSON comme l'écrit le Monteur 1 (`json.dump(…, ensure_ascii=False, indent=1)`).
pub fn json_indente<T: Serialize>(valeur: &T) -> serde_json::Result<String> {
    let mut sortie = Vec::new();
    let mut ecrivain =
        serde_json::Serializer::with_formatter(&mut sortie, serde_json::ser::PrettyFormatter::with_indent(b" "));
    valeur.serialize(&mut ecrivain)?;
    Ok(String::from_utf8(sortie).expect("serde_json écrit de l'UTF-8"))
}

pub fn ecrire_atomique(chemin: &Path, prefixe: &str, contenu: &[u8]) -> std::io::Result<()> {
    let dossier = chemin.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dossier)?;
    let temp = dossier.join(format!(
        ".{prefixe}-{}-{}",
        std::process::id(),
        COMPTEUR.fetch_add(1, Ordering::Relaxed)
    ));
    let resultat = (|| {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(contenu)?;
        f.sync_all()?;
        std::fs::rename(&temp, chemin)
    })();
    if resultat.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    resultat
}

pub fn ecrire_json<T: Serialize>(chemin: &Path, prefixe: &str, valeur: &T) -> std::io::Result<()> {
    let texte = json_indente(valeur).map_err(std::io::Error::other)?;
    ecrire_atomique(chemin, prefixe, texte.as_bytes())
}
