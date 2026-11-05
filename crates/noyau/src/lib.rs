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
pub mod prereglages;
pub mod projet;
pub mod projets;
pub mod sous_titres;
pub mod temps_morts;
pub mod verrou;
pub mod videos;
pub mod voix;

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
/// Fichiers que le Monteur fabrique à partir d'une vidéo : ce ne sont pas des sources.
pub const SUFFIXES_DERIVES: [&str; 4] = [" avec vignette", " montage YouTube", " court", " brouillon"];
/// Ce qui accompagne une vidéo et part avec elle quand on la range.
pub const SUFFIXE_COURT: &str = " court.mp4";
pub const SUFFIXE_BROUILLON: &str = " brouillon.mp4";
pub const SUFFIXE_GIF: &str = " extrait.gif";
pub const SUFFIXES_LIES: [&str; 12] = [
    SUFFIXE_PNG,
    SUFFIXE_JPG,
    SUFFIXE_MONTEE,
    SUFFIXE_DESCRIPTION,
    SUFFIXE_MONTAGE,
    SUFFIXE_PROJET,
    EXTENSION_PROJET,
    SUFFIXE_SRT_MONTAGE,
    SUFFIXE_SRT,
    SUFFIXE_COURT,
    SUFFIXE_BROUILLON,
    SUFFIXE_GIF,
];
pub const EXTENSIONS: [&str; 3] = ["mp4", "mkv", "mov"];
/// La vignette : 1280 × 720, l'image de la vidéo à droite sur 646 × 720 (charte du 23/10/2026).
pub const VIGNETTE_L: i64 = 1280;
pub const VIGNETTE_H: i64 = 720;
pub const IMAGE_L: i64 = 646;
pub const IMAGE_H: i64 = 720;
pub const FOND_MONTAGE: &str = "0x121214";
pub const DUREE_VIGNETTE: f64 = 3.0;

pub use videos::chemin_derive;

/// L'heure qu'il est, comme le Monteur 1 note un envoi (`isoformat(timespec="seconds")`).
pub fn maintenant() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
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
