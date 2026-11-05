//! Les préréglages de rendu. Le premier est le rendu de toujours.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prereglage {
    pub nom: &'static str,
    pub detail: &'static str,
    /// Le fichier de sortie, à côté de la prise.
    pub suffixe: &'static str,
    /// Filtre d'image ffmpeg, sur l'image peinte.
    pub image: Option<&'static str>,
    /// Réglages x264 ; `None` : pas de H.264 (GIF).
    pub x264: Option<(&'static str, &'static str)>,
    pub son: bool,
    /// Vignette en tête et sous-titres à part : la vraie sortie pour YouTube.
    pub youtube: bool,
    pub duree_max: Option<f64>,
}

pub const PREREGLAGES: [Prereglage; 5] = [
    Prereglage {
        nom: "YouTube",
        detail: "taille de la prise · H.264 · AAC 192k",
        suffixe: crate::SUFFIXE_MONTAGE,
        image: None,
        x264: Some(("18", "medium")),
        son: true,
        youtube: true,
        duree_max: None,
    },
    Prereglage {
        nom: "YouTube 1440p",
        detail: "agrandie : mieux compressée par YouTube",
        suffixe: crate::SUFFIXE_MONTAGE,
        image: Some("scale=-2:1440:flags=lanczos"),
        x264: Some(("18", "medium")),
        son: true,
        youtube: true,
        duree_max: None,
    },
    Prereglage {
        nom: "Court vertical 9:16",
        detail: "1080 × 1920 · centre de l'image · 60 s au plus",
        suffixe: crate::SUFFIXE_COURT,
        image: Some("crop=ih*9/16:ih,scale=1080:1920:flags=lanczos"),
        x264: Some(("20", "medium")),
        son: true,
        youtube: false,
        duree_max: Some(60.0),
    },
    Prereglage {
        nom: "Brouillon rapide",
        detail: "720p · pour se relire",
        suffixe: crate::SUFFIXE_BROUILLON,
        image: Some("scale=-2:720:flags=bilinear"),
        x264: Some(("28", "veryfast")),
        son: true,
        youtube: false,
        duree_max: None,
    },
    Prereglage {
        nom: "Extrait GIF",
        detail: "800 px · 15 i/s · sans son · 15 s au plus",
        suffixe: crate::SUFFIXE_GIF,
        image: Some("fps=15,scale=800:-2:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse"),
        x264: None,
        son: false,
        youtube: false,
        duree_max: Some(15.0),
    },
];
