//! Le cœur du Monteur, sans interface ni écran : tout ce qui se teste seul.
//!
//! Les fichiers lus et écrits sont ceux du Monteur 1 (PySide6), au même format : les deux
//! Monteurs travaillent sur les mêmes prises tant que le premier reste en service. Un champ que ce
//! Monteur ne connaît pas est gardé tel quel et réécrit, jamais perdu.

pub mod analyse;
pub mod arrondi;
pub mod description;
pub mod ecriture;
pub mod fiches;
pub mod journal;
pub mod montage;
pub mod projet;
pub mod sous_titres;
pub mod temps_morts;

use std::path::{Path, PathBuf};

// Les fichiers posés à côté d'une vidéo (`noyau.SUFFIXE_*` du Monteur 1).
pub const SUFFIXE_PNG: &str = " vignette YouTube.png";
pub const SUFFIXE_JPG: &str = " vignette YouTube.jpg";
pub const SUFFIXE_MONTEE: &str = " avec vignette.mp4";
pub const SUFFIXE_DESCRIPTION: &str = " description YouTube.txt";
pub const SUFFIXE_MONTAGE: &str = " montage YouTube.mp4";
pub const SUFFIXE_PROJET: &str = " montage.json";
pub const EXTENSION_PROJET: &str = ".monteur";
pub const SUFFIXE_SRT_MONTAGE: &str = " montage YouTube.fr.srt";
pub const SUFFIXE_SRT: &str = " avec vignette.fr.srt";

/// « <dossier>/<nom de la vidéo sans extension><suffixe> ».
pub fn chemin_derive(video: &Path, suffixe: &str) -> PathBuf {
    let nom = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    video.with_file_name(format!("{nom}{suffixe}"))
}

/// Une erreur de lecture, en français, pour la montrer telle quelle.
#[derive(Debug)]
pub struct Erreur(pub String);

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Erreur {}
