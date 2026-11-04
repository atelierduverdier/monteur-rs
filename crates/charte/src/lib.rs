//! La charte de VerdierCAM (`verdiercam-imgui/src/app/Charte.h`), recopiée pour que le Monteur et
//! VerdierCAM se ressemblent. Aucune couleur d'interface ne s'écrit en dur ailleurs que dans ce
//! fichier.

use dear_imgui_rs::{Style, StyleColor};

// Neutres sombres
pub const BG_CANVAS: u32 = 0x121214; // fond de la visionneuse
pub const BG_TITLE: u32 = 0x181818; // titres, onglets inactifs
pub const BG_PANEL: u32 = 0x1E1E1E; // panneaux
pub const BG_CARD: u32 = 0x242424; // cartes, popups, en-têtes
pub const BG_INPUT: u32 = 0x2B2B2B; // champs
pub const BORDER_OUTER: u32 = 0x101010;
pub const BORDER: u32 = 0x333333;
pub const TEXT: u32 = 0xE6E6E6;
pub const TEXT_2: u32 = 0xA8A8A8; // libellés, icônes au repos
pub const TEXT_DISABLED: u32 = 0x6E6E6E;
pub const UNIT: u32 = 0x8A8A8A; // unités, temps secondaires

// Accent : orange industriel
pub const ACCENT: u32 = 0xFF6D00;
pub const ACCENT_HOVER: u32 = 0xFF8F00;
pub const ACCENT_PRESSED: u32 = 0xE65100;
pub const CHIFFRE_SAISIE: u32 = 0xFF9100; // le temps de la tête de lecture
pub const ENCRE_SUR_ACCENT: u32 = 0x121214;

pub const BG_HOVER: u32 = 0x333333;
pub const BG_PRESSED: u32 = 0x3A3A3A;
pub const BARRE_SURVOL: u32 = 0x4A4A4A;

// Sémantique
pub const SUCCES: u32 = 0x4CAF50;
pub const SUCCES_TEXTE: u32 = 0x86E08A;
pub const ALERTE: u32 = 0xFFC107;
pub const ERREUR: u32 = 0xF44336;

// Pistes de la ligne de temps (maquette Design du 04/11/2026)
pub const PISTE_ZOOM: u32 = 0x2979FF;
pub const PISTE_ETIQUETTE: u32 = ACCENT;
pub const PISTE_VOIX: u32 = SUCCES;
pub const PISTE_MUSIQUE: u32 = 0x9575CD;
pub const PISTE_COUPE: u32 = ERREUR;
pub const MARQUEUR: u32 = ALERTE;

/// Une couleur `0xRRGGBB` en flottants pour ImGui.
pub fn vec(rgb: u32, alpha: f32) -> [f32; 4] {
    [
        ((rgb >> 16) & 0xFF) as f32 / 255.0,
        ((rgb >> 8) & 0xFF) as f32 / 255.0,
        (rgb & 0xFF) as f32 / 255.0,
        alpha,
    ]
}

/// La même, opaque.
pub fn c(rgb: u32) -> [f32; 4] {
    vec(rgb, 1.0)
}

/// Pose la charte sur tout le style d'ImGui.
pub fn appliquer(style: &mut Style) {
    use StyleColor as S;
    let couleurs = [
        (S::Text, c(TEXT)),
        (S::TextDisabled, c(TEXT_DISABLED)),
        (S::WindowBg, c(BG_PANEL)),
        (S::ChildBg, vec(BG_PANEL, 0.0)),
        (S::PopupBg, c(BG_CARD)),
        (S::Border, c(BORDER)),
        (S::BorderShadow, vec(0, 0.0)),
        (S::FrameBg, c(BG_INPUT)),
        (S::FrameBgHovered, c(BG_HOVER)),
        (S::FrameBgActive, c(BG_PRESSED)),
        (S::TitleBg, c(BG_TITLE)),
        (S::TitleBgActive, c(BG_TITLE)),
        (S::TitleBgCollapsed, c(BG_TITLE)),
        (S::MenuBarBg, c(BG_TITLE)),
        (S::ScrollbarBg, vec(BG_PANEL, 0.0)),
        (S::ScrollbarGrab, c(BG_PRESSED)),
        (S::ScrollbarGrabHovered, c(BARRE_SURVOL)),
        (S::ScrollbarGrabActive, c(BARRE_SURVOL)),
        (S::CheckMark, c(ACCENT)),
        (S::SliderGrab, c(ACCENT)),
        (S::SliderGrabActive, c(ACCENT_HOVER)),
        (S::Button, c(BG_INPUT)),
        (S::ButtonHovered, c(BG_HOVER)),
        (S::ButtonActive, c(BG_PRESSED)),
        (S::Header, vec(ACCENT, 0.15)),
        (S::HeaderHovered, vec(ACCENT, 0.25)),
        (S::HeaderActive, vec(ACCENT, 0.35)),
        (S::Separator, c(BORDER)),
        (S::SeparatorHovered, c(ACCENT_HOVER)),
        (S::SeparatorActive, c(ACCENT)),
        (S::ResizeGrip, vec(0, 0.0)),
        (S::ResizeGripHovered, c(ACCENT_HOVER)),
        (S::ResizeGripActive, c(ACCENT)),
        (S::Tab, c(BG_TITLE)),
        (S::TabHovered, c(BG_HOVER)),
        (S::TabSelected, c(BG_PANEL)),
        (S::TabSelectedOverline, c(ACCENT)),
        (S::TabDimmed, c(BG_TITLE)),
        (S::TabDimmedSelected, c(BG_PANEL)),
        (S::TabDimmedSelectedOverline, vec(ACCENT, 0.5)),
        (S::DockingPreview, vec(ACCENT, 0.4)),
        (S::DockingEmptyBg, c(BG_CANVAS)),
        (S::TableHeaderBg, c(BG_CARD)),
        (S::TableBorderStrong, c(BORDER)),
        (S::TableBorderLight, c(BG_CARD)),
        (S::TextSelectedBg, vec(ACCENT, 0.30)),
        (S::NavCursor, c(ACCENT_HOVER)),
        (S::ModalWindowDimBg, vec(0, 0.55)),
    ];
    for (nom, valeur) in couleurs {
        style.set_color(nom, valeur);
    }
    style.set_window_rounding(0.0);
    style.set_frame_rounding(4.0);
}
