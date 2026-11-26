//! Le menu Aide : « Ce qu'il faut » (chaque outil du Monteur, présent ou non, et de quoi réparer ce qui manque),
//! le mode d'emploi, « À propos ». « Ce qu'il faut » s'ouvre seul au premier lancement, ou quand ffmpeg manque :
//! sans lui, rien ne s'ouvre, et le dire vaut mieux qu'une liste vide.

use std::path::PathBuf;

use dear_imgui_rs::{Ui, WindowFlags};

/// Le code source, à donner avec le programme (GPL).
pub const SOURCES: &str = "https://github.com/atelierduverdier/monteur-verdiercam";
/// Le mode d'emploi en ligne, quand il n'est pas à côté du programme.
const MODE_EMPLOI_EN_LIGNE: &str = "https://github.com/atelierduverdier/monteur-verdiercam/blob/main/docs/mode-emploi.md";

fn chemin_accueil() -> PathBuf {
    noyau::systeme::donnees().join("accueil-vu")
}

/// « Ce qu'il faut » s'ouvre seul : premier lancement, ou ffmpeg introuvable.
pub fn a_montrer_au_lancement() -> bool {
    !chemin_accueil().exists() || noyau::systeme::trouver_programme("ffmpeg").is_none()
}

/// Le mode d'emploi livré : à côté du programme (Windows), ou dans `share/` (AppImage) ; en ligne sinon.
pub fn ouvrir_le_mode_d_emploi() -> Result<(), String> {
    let dossier = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::Path::to_path_buf));
    let local = dossier.and_then(|d| {
        [d.join("Mode d'emploi.html"), d.join("../share/monteur-verdiercam/Mode d'emploi.html")].into_iter().find(|p| p.is_file())
    });
    match local {
        Some(p) => opener::open(p).map_err(|e| e.to_string()),
        None => opener::open_browser(MODE_EMPLOI_EN_LIGNE).map_err(|e| e.to_string()),
    }
}

/// Une ligne : bon ou à faire, ce que c'est, le détail.
fn ligne(ui: &Ui, bon: bool, quoi: &str, detail: &str) {
    ui.text_colored(charte::c(if bon { charte::SUCCES_TEXTE } else { charte::ALERTE }), if bon { "✓" } else { "!" });
    ui.same_line_with_pos(26.0);
    ui.text(quoi);
    ui.same_line_with_pos(250.0);
    ui.text_colored(charte::c(charte::TEXT_2), detail);
}

fn ou(p: Option<PathBuf>) -> String {
    p.map_or_else(String::new, |p| p.display().to_string())
}

impl crate::Monteur {
    pub(crate) fn boite_ce_qu_il_faut(&mut self, ui: &Ui) {
        const TITRE: &str = "Ce qu'il faut";
        if std::mem::take(&mut self.ouvrir_ce_qu_il_faut) {
            ui.open_popup(TITRE);
        }
        let Some(boite) = ui.begin_modal_popup_config(TITRE).flags(WindowFlags::ALWAYS_AUTO_RESIZE).begin() else { return };
        ui.text_wrapped("Le Monteur VerdierCAM filme VerdierCAM, prépare le montage d'après ses gestes, sous-titre la voix et rend la vidéo. Voici ce qu'il a trouvé sur ce poste.");
        ui.spacing();
        let ffmpeg = noyau::systeme::trouver_programme("ffmpeg");
        let ffprobe = noyau::systeme::trouver_programme("ffprobe");
        let image = ffmpeg.is_some() && ffprobe.is_some();
        ligne(ui, image, "Image et son", &if image { ou(ffmpeg) } else { "ffmpeg ou ffprobe introuvable : sans eux, rien ne s'ouvre".into() });
        if cfg!(windows) {
            ligne(ui, true, "Filmer", "intégré (Windows 10 ou 11)");
        } else {
            let gsr = noyau::systeme::trouver_programme("gpu-screen-recorder");
            let detail = if gsr.is_some() { ou(gsr) } else { "gpu-screen-recorder à installer (paquet de la distribution, ou Flatpak)".into() };
            ligne(ui, gsr_present(), "Filmer", &detail);
        }
        let whisper = noyau::systeme::trouver_programme("whisper-cli");
        ligne(ui, whisper.is_some(), "Sous-titres", &if whisper.is_some() { ou(whisper) } else { "whisper-cli introuvable : pas de sous-titres".into() });
        let modele = noyau::sous_titres::trouver_modele();
        // L'avancement ici aussi (23/11/2026, Christophe : « je n'ai plus d'info pour savoir si ça charge ou pas ») : la
        // barre d'état n'est pas toujours en vue.
        let detail = match (&modele, &self.telechargement) {
            (Some(_), _) => ou(modele.clone()),
            (None, Some(t)) => format!("téléchargement en cours : {:.0} % de 575 Mo", t.avance() * 100.0),
            (None, None) => "à télécharger (575 Mo)".into(),
        };
        ligne(ui, modele.is_some(), "Modèle des sous-titres", &detail);
        if modele.is_none() && self.telechargement.is_none() {
            ui.same_line();
            if ui.small_button("Télécharger…") && self.telechargement.is_none() {
                ui.close_current_popup();
                self.ouvrir_modele = true;
            }
        }
        let langue = noyau::sous_titres::langue();
        let nom_langue = noyau::sous_titres::LANGUES.iter().find(|(c, _)| *c == langue).map_or("français", |(_, n)| n);
        ligne(ui, true, "Langue des sous-titres", &format!("{nom_langue} (Montage › Langue des sous-titres)"));
        let verdiercam = noyau::systeme::verdiercam();
        let trouve = verdiercam.is_file();
        ligne(ui, trouve, "VerdierCAM", &if trouve { verdiercam.display().to_string() } else { "introuvable : Filmer VerdierCAM le demandera".into() });
        if !trouve {
            ui.same_line();
            if ui.small_button("Choisir…") {
                self.verdiercam_choisi = Some(Self::dialogue(noyau::systeme::maison(), "Où est VerdierCAM ?", crate::FILTRE_VERDIERCAM));
            }
        }
        let direct = crate::youtube::Youtube::envoi_direct();
        ligne(ui, true, "YouTube", if direct { "envoi direct, en privé (youtube-envoi)" } else { "préparé pour YouTube Studio (onglet YouTube)" });
        ui.spacing();
        ui.text_colored(charte::c(charte::TEXT_2), format!("Prises : {}   ·   Projets : {}", self.copies.display(), self.racine.display()));
        ui.spacing();
        if ui.button_with_size("Mode d'emploi", [150.0, 30.0])
            && let Err(e) = ouvrir_le_mode_d_emploi()
        {
            self.message = format!("Mode d'emploi non ouvert : {e}");
        }
        ui.same_line();
        if ui.button_with_size("Fermer##ce_qu_il_faut", [100.0, 30.0]) {
            ui.close_current_popup();
            let accueil = chemin_accueil();
            if let Some(d) = accueil.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(accueil, "");
        }
        boite.end();
    }

    pub(crate) fn boite_a_propos(&mut self, ui: &Ui) {
        const TITRE: &str = "À propos du Monteur VerdierCAM";
        if std::mem::take(&mut self.ouvrir_a_propos) {
            ui.open_popup(TITRE);
        }
        let Some(boite) = ui.begin_modal_popup_config(TITRE).flags(WindowFlags::ALWAYS_AUTO_RESIZE).begin() else { return };
        ui.text(format!("Monteur VerdierCAM {}", env!("CARGO_PKG_VERSION")));
        ui.spacing();
        ui.text_wrapped("Logiciel libre, sous licence GPL version 3 ou suivante : chacun peut l'étudier, le modifier et le redistribuer.");
        ui.text_colored(charte::c(charte::TEXT_2), SOURCES);
        ui.text_wrapped("Le nom et le logo VerdierCAM restent la marque de l'Atelier du Verdier.");
        ui.text_wrapped("Il s'appuie sur FFmpeg, whisper.cpp, Dear ImGui, wgpu et cpal ; leurs licences sont dans le dossier licences.");
        ui.spacing();
        if ui.button_with_size("Code source", [150.0, 30.0]) {
            let _ = opener::open_browser(SOURCES);
        }
        ui.same_line();
        if ui.button_with_size("Fermer##a_propos", [100.0, 30.0]) {
            ui.close_current_popup();
        }
        boite.end();
    }
}

/// gpu-screen-recorder, ou le programme qui le remplace pour les essais (`MONTEUR2_ECRAN`).
fn gsr_present() -> bool {
    std::env::var_os("MONTEUR2_ECRAN").is_some() || noyau::systeme::trouver_programme("gpu-screen-recorder").is_some()
}
