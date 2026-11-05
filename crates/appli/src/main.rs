//! Le Monteur 2 : la fenêtre. Prises à gauche, visionneuse au milieu, inspecteur à droite, ligne de
//! temps en bas (la maquette Design du 04/11/2026). Le montage s'écrit seul à côté de la vidéo,
//! au même format que le Monteur 1 ; la vidéo n'est jamais touchée.
//!
//! `monteur2 [dossier]` : sans dossier, celui des fiches du Monteur 1, sinon « Copies d'écran ».

mod etat;
mod ligne;
mod miniatures;
mod scenario;
mod vue;

use std::path::PathBuf;

use composeur::police::Polices;
use dear_app::{
    AppConfig, Application, ApplicationStage, DockingConfig, FrameContext, GpuContext, InitContext, InitializedContext,
    RedrawMode, RunError, WgpuConfig, WgpuPreset, run,
};
use dear_imgui_rs::*;
use noyau::fiches::Fiches;

use etat::{Choix, Dossier, Prise, Sorte, temps};

const INTER: &[u8] = include_bytes!("../../../ressources/polices/Inter-Regular.ttf");
const INTER_GRAS: &[u8] = include_bytes!("../../../ressources/polices/Inter-SemiBold.ttf");
const MONO: &[u8] = include_bytes!("../../../ressources/polices/JetBrainsMono-Medium.ttf");

struct Panneaux {
    prises: WindowKey,
    visionneuse: WindowKey,
    inspecteur: WindowKey,
    ligne: WindowKey,
}

impl Panneaux {
    fn new() -> Self {
        let cle = |id, titre| WindowKey::new(id, titre).expect("clé de panneau");
        Self {
            prises: cle("prises", "Prises"),
            visionneuse: cle("visionneuse", "Visionneuse"),
            inspecteur: cle("inspecteur", "Inspecteur"),
            ligne: cle("ligne", "Ligne de temps"),
        }
    }

    fn disposition(&self) -> DockLayout {
        DockLayout::split(
            DockSplit::Down,
            0.30,
            DockLayout::tabs([&self.ligne]),
            DockLayout::split(
                DockSplit::Left,
                0.17,
                DockLayout::tabs([&self.prises]),
                DockLayout::split(DockSplit::Right, 0.24, DockLayout::tabs([&self.inspecteur]), DockLayout::tabs([&self.visionneuse])),
            ),
        )
    }
}

/// Ce que la souris tient dans la visionneuse quand un zoom est choisi.
#[derive(Clone, Copy)]
enum Saisie {
    /// Le cadre entier : départ de la souris, centre de départ.
    Deplacer([f32; 2], f64, f64),
    /// Un coin, ou un cadre neuf qu'on trace : le point fixe, en fractions de l'image.
    Tracer(f64, f64),
}

/// Les boutons de lecture, dessinés (Inter n'a pas ces signes en assez net).
#[derive(Clone, Copy)]
enum Forme {
    Lire,
    Pause,
    Stop,
    Debut,
    Fin,
    Reculer,
    Avancer,
}

/// Un bouton carré à icône ; rend vrai s'il est cliqué.
fn bouton_forme(ui: &Ui, id: &str, forme: Forme, primaire: bool, aide: &str) -> bool {
    let couleurs = primaire.then(|| {
        (
            ui.push_style_color(StyleColor::Button, charte::c(charte::ACCENT)),
            ui.push_style_color(StyleColor::ButtonHovered, charte::c(charte::ACCENT_HOVER)),
            ui.push_style_color(StyleColor::ButtonActive, charte::c(charte::ACCENT_PRESSED)),
        )
    });
    let clique = ui.button_with_size(format!("##{id}"), [if primaire { 46.0 } else { 34.0 }, 28.0]);
    if let Some((a, b, c)) = couleurs {
        c.pop();
        b.pop();
        a.pop();
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(aide);
    }
    let (a, b) = (ui.item_rect_min(), ui.item_rect_max());
    let (cx, cy) = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    let encre = charte::c(if primaire { charte::ENCRE_SUR_ACCENT } else { charte::TEXT });
    let dl = ui.get_window_draw_list();
    let plein = |x0: f32, y0: f32, x1: f32, y1: f32| dl.add_rect([cx + x0, cy + y0], [cx + x1, cy + y1], encre).filled(true).build();
    let fleche = |x: f32, sens: f32| {
        dl.add_triangle([cx + x - 5.0 * sens, cy - 6.0], [cx + x - 5.0 * sens, cy + 6.0], [cx + x + 5.0 * sens, cy], encre).filled(true).build()
    };
    match forme {
        Forme::Lire => fleche(1.0, 1.0),
        Forme::Pause => {
            plein(-5.0, -6.0, -1.5, 6.0);
            plein(1.5, -6.0, 5.0, 6.0);
        }
        Forme::Stop => plein(-5.0, -5.0, 5.0, 5.0),
        Forme::Debut => {
            plein(-7.0, -6.0, -5.0, 6.0);
            fleche(0.0, -1.0);
        }
        Forme::Fin => {
            plein(5.0, -6.0, 7.0, 6.0);
            fleche(0.0, 1.0);
        }
        Forme::Reculer => {
            fleche(-4.0, -1.0);
            fleche(4.0, -1.0);
        }
        Forme::Avancer => {
            fleche(-4.0, 1.0);
            fleche(4.0, 1.0);
        }
    }
    clique
}

/// La hauteur du ruban, sous la barre de menus.
const RUBAN: f32 = 44.0;

struct Monteur {
    panneaux: Panneaux,
    dossier: Dossier,
    prise: Option<Prise>,
    vue: vue::Vue,
    ligne: ligne::Ligne,
    polices: Polices,
    message: String,
    gras: Option<FontId>,
    mono: Option<FontId>,
    /// Une prise à ouvrir au tour suivant (le clic arrive pendant le dessin de la liste).
    a_ouvrir: Option<PathBuf>,
    /// Le réglage d'un zoom à l'arrêt : ce que la souris tient dans la visionneuse.
    cadre_saisi: Option<Saisie>,
    miniatures: miniatures::Miniatures,
    /// Un déroulé d'essai (`MONTEUR2_SCENARIO`), sinon rien.
    scenario: Option<scenario::Scenario>,
}

fn dossier_de_depart(fiches: &Fiches) -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| fiches.dossier.clone())
        .unwrap_or_else(noyau::videos::dossier_defaut)
}

fn chemin_fiches() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".local/share"))
        .join("monteur/fiches.json")
}

impl Monteur {
    fn new() -> Self {
        // Les fiches ne sont que lues ici : le Monteur 1 reste celui qui les écrit.
        let fiches = Fiches::lire(&chemin_fiches()).unwrap_or_default();
        let dossier = Dossier::lire(&dossier_de_depart(&fiches), &fiches);
        let message = format!("{} prise(s) dans {}", dossier.prises.len(), dossier.chemin.display());
        let miniatures = miniatures::Miniatures::lancer(dossier.prises.iter().map(|p| p.0.clone()).collect());
        Self {
            panneaux: Panneaux::new(),
            dossier,
            prise: None,
            vue: vue::Vue::new(),
            ligne: ligne::Ligne::new(),
            polices: Polices::charger(),
            message,
            gras: None,
            mono: None,
            a_ouvrir: None,
            cadre_saisi: None,
            miniatures,
            scenario: scenario::Scenario::depuis_l_environnement(),
        }
    }

    fn ouvrir(&mut self, video: PathBuf) {
        if let Some(p) = &mut self.prise
            && let Some(e) = p.ecrire_si_besoin(true)
        {
            self.message = e;
        }
        self.prise = None;
        self.vue.oublier_source();
        match Prise::ouvrir(&video) {
            Ok(p) => {
                self.message = format!(
                    "{} : {} × {}, {} ; {} zoom(s), {} étiquette(s), {} sous-titre(s), {} coupe(s)",
                    video.file_name().unwrap_or_default().to_string_lossy(),
                    p.infos.largeur,
                    p.infos.hauteur,
                    temps(p.infos.duree),
                    p.montage.zooms.len(),
                    p.montage.etiquettes.len(),
                    p.montage.sous_titres.len(),
                    p.montage.coupes.len()
                );
                self.prise = Some(p);
            }
            Err(e) => self.message = format!("{} ne s'ouvre pas : {e}", video.display()),
        }
    }

    /// Les touches, quand on n'écrit pas dans un champ.
    fn touches(&mut self, ui: &Ui) {
        if ui.io().want_text_input() {
            return;
        }
        let Some(p) = &mut self.prise else { return };
        let (ctrl, maj) = (ui.io().key_ctrl(), ui.io().key_shift());
        let image = 1.0 / 30.0;
        let t = p.lecteur.position();
        if ui.is_key_pressed(Key::Space) {
            p.lecteur.basculer();
        }
        if ui.is_key_pressed(Key::K) {
            p.lecteur.arreter();
        }
        if ui.is_key_pressed(Key::Z) && !ctrl {
            p.nouveau_zoom(t);
            self.message = "Zoom posé : trace son cadre dans la visionneuse.".into();
        }
        if ui.is_key_pressed(Key::J) {
            p.lecteur.aller_a(t - 5.0);
        }
        if ui.is_key_pressed(Key::L) {
            p.lecteur.aller_a(t + 5.0);
        }
        if ui.is_key_pressed(Key::LeftArrow) {
            p.lecteur.aller_a(t - if maj { 1.0 } else { image });
        }
        if ui.is_key_pressed(Key::RightArrow) {
            p.lecteur.aller_a(t + if maj { 1.0 } else { image });
        }
        if ui.is_key_pressed(Key::Home) {
            p.lecteur.aller_a(0.0);
        }
        if ui.is_key_pressed(Key::End) {
            p.lecteur.aller_a(p.montage.duree);
        }
        if ui.is_key_pressed(Key::C) && !ctrl {
            self.message = p.marquer_coupe(t);
        }
        if ui.is_key_pressed(Key::Escape) {
            p.coupe_commencee = None;
            p.choix = None;
        }
        if ui.is_key_pressed(Key::Delete) {
            p.supprimer_choix();
        }
        if ctrl && ui.is_key_pressed(Key::Z) {
            if maj { p.refaire() } else { p.defaire() }
        }
        if ctrl && ui.is_key_pressed(Key::Y) {
            p.refaire();
        }
        if ctrl && ui.is_key_pressed(Key::S)
            && let Some(e) = p.ecrire_si_besoin(true)
        {
            self.message = e;
        }
    }

    fn panneau_prises(&mut self, ui: &Ui, textures: &[Option<TextureId>]) {
        let ouverte = self.prise.as_ref().map(|p| p.video.clone());
        const HAUT: f32 = 58.0;
        let (mh, ml) = (46.0, 46.0 * miniatures::LARGEUR as f32 / miniatures::HAUTEUR as f32);
        for (i, (chemin, etat)) in self.dossier.prises.iter().enumerate() {
            if ui.selectable_config(format!("##prise{i}")).selected(ouverte.as_ref() == Some(chemin)).size([0.0, HAUT]).build() {
                self.a_ouvrir = Some(chemin.clone());
            }
            let (a, b) = (ui.item_rect_min(), ui.item_rect_max());
            let dl = ui.get_window_draw_list();
            let (x, y) = (a[0] + 4.0, a[1] + (HAUT - mh) / 2.0);
            match textures.get(i).copied().flatten() {
                Some(t) => dl.add_image(t, [x, y], [x + ml, y + mh], [0.0, 0.0], [1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
                None => dl.add_rect([x, y], [x + ml, y + mh], charte::c(charte::BG_INPUT)).filled(true).build(),
            }
            let nom = chemin.file_stem().unwrap_or_default().to_string_lossy();
            let tx = x + ml + 8.0;
            dl.add_text([tx, a[1] + 8.0], charte::c(charte::TEXT), miniatures::nom_lisible(&nom));
            if let Some(d) = self.miniatures.duree(i) {
                dl.add_text([tx, a[1] + 31.0], charte::c(charte::UNIT), temps(d).split(',').next().unwrap_or(""));
            }
            // L'état à droite, en pastille, de la couleur de la maquette.
            let (fond, encre) = match *etat {
                "montée" => (charte::vec(charte::PISTE_ZOOM, 0.25), 0x8DB5FF),
                "vignette" => (charte::vec(charte::ALERTE, 0.2), 0xFFD54F),
                "envoyée" => (charte::vec(charte::SUCCES, 0.22), charte::SUCCES_TEXTE),
                _ => (charte::c(charte::BG_INPUT), charte::TEXT_2),
            };
            let l = ui.calc_text_size(etat)[0] + 14.0;
            let (px, py) = (b[0] - l - 6.0, a[1] + 30.0);
            dl.add_rect([px, py], [px + l, py + 19.0], fond).filled(true).rounding(9.0).build();
            dl.add_text([px + 7.0, py + 1.0], charte::c(encre), etat);
        }
        if self.dossier.prises.is_empty() {
            ui.text_wrapped(format!("Aucune vidéo dans {}", self.dossier.chemin.display()));
        }
    }

    /// Sous la barre de menus : ce qu'on fait le plus souvent, en vue.
    fn ruban(&mut self, ui: &Ui) {
        let pret = self.prise.as_ref().is_some_and(|p| p.preparation.is_none());
        let primaire = (
            ui.push_style_color(StyleColor::Button, charte::c(charte::ACCENT)),
            ui.push_style_color(StyleColor::ButtonHovered, charte::c(charte::ACCENT_HOVER)),
            ui.push_style_color(StyleColor::ButtonActive, charte::c(charte::ACCENT_PRESSED)),
            ui.push_style_color(StyleColor::Text, charte::c(charte::ENCRE_SUR_ACCENT)),
        );
        let preparer = {
            let _f = self.gras.map(|f| ui.push_font(f));
            ui.button_with_size("Préparer le montage", [170.0, 30.0])
        };
        primaire.3.pop();
        primaire.2.pop();
        primaire.1.pop();
        primaire.0.pop();
        if preparer
            && pret
            && let Some(p) = &mut self.prise
        {
            p.preparer();
            self.message = "Préparation en cours : mouvement de l'image, gestes, silences…".into();
        }
        let Some(p) = &mut self.prise else { return };
        let t = p.lecteur.position();
        ui.same_line();
        ui.dummy([12.0, 1.0]);
        ui.same_line();
        if ui.button_with_size("Annuler", [0.0, 30.0]) {
            p.defaire();
        }
        ui.same_line();
        if ui.button_with_size("Rétablir", [0.0, 30.0]) {
            p.refaire();
        }
        ui.same_line();
        ui.dummy([12.0, 1.0]);
        ui.same_line();
        if ui.button_with_size("Couper (C)", [0.0, 30.0]) {
            self.message = p.marquer_coupe(t);
        }
        ui.same_line();
        if ui.button_with_size("Couper avant", [0.0, 30.0]) {
            p.ajouter_coupe(0.0, t);
        }
        ui.same_line();
        if ui.button_with_size("Couper après", [0.0, 30.0]) {
            p.ajouter_coupe(t, p.montage.duree);
        }
        ui.same_line();
        ui.dummy([12.0, 1.0]);
        ui.same_line();
        if ui.button_with_size("Nouveau zoom (Z)", [0.0, 30.0]) {
            p.nouveau_zoom(t);
            self.message = "Zoom posé : trace son cadre dans la visionneuse.".into();
        }
    }

    fn panneau_visionneuse(&mut self, ui: &Ui, image: Option<TextureId>) {
        let dispo = ui.content_region_avail();
        self.vue.taille = [dispo[0].max(64.0) as u32, (dispo[1] - 44.0).max(36.0) as u32];
        let Some(p) = &mut self.prise else {
            ui.text_colored(charte::c(charte::TEXT_2), "Choisir une prise à gauche.");
            return;
        };
        if let (Some(id), Some(taille)) = (image, self.vue.taille_image()) {
            let x0 = ui.cursor_pos()[0] + (dispo[0] - taille[0]).max(0.0) / 2.0;
            ui.set_cursor_pos([x0, ui.cursor_pos()[1]]);
            let coin = ui.cursor_screen_pos();
            ui.image(id, taille);
            // Un zoom choisi, à l'arrêt : l'image entière et son cadre orange. Glisser dedans le
            // déplace ; un coin le retaille ; tracer à côté en fait un autre ; la molette grossit.
            if let Some(Choix { sorte: Sorte::Zoom, indice }) = p.choix
                && !p.lecteur.en_lecture()
                && let Some(z) = p.montage.zooms.get(indice).cloned()
            {
                let [x, y, l, h] = noyau::analyse::cadre_zoom(z.facteur, z.cx, z.cy);
                let a = [coin[0] + x as f32 * taille[0], coin[1] + y as f32 * taille[1]];
                let b = [a[0] + l as f32 * taille[0], a[1] + h as f32 * taille[1]];
                let dl = ui.get_window_draw_list();
                dl.add_rect(a, b, charte::c(charte::ACCENT)).thickness(3.0).build();
                let coins = [[a[0], a[1]], [b[0], a[1]], [a[0], b[1]], [b[0], b[1]]];
                for c in coins {
                    dl.add_rect([c[0] - 5.0, c[1] - 5.0], [c[0] + 5.0, c[1] + 5.0], charte::c(charte::ACCENT)).filled(true).build();
                }
                let souris = ui.io().mouse_pos();
                let (fx, fy) = (((souris[0] - coin[0]) / taille[0]) as f64, ((souris[1] - coin[1]) / taille[1]) as f64);
                let (fx, fy) = (fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0));
                let proche = coins.iter().position(|c| (c[0] - souris[0]).abs() <= 10.0 && (c[1] - souris[1]).abs() <= 10.0);
                if ui.is_item_hovered() {
                    if proche.is_some() {
                        ui.set_mouse_cursor(Some(MouseCursor::ResizeNWSE));
                    }
                    if ui.is_mouse_clicked(MouseButton::Left) {
                        p.retenir();
                        let dedans = souris[0] >= a[0] && souris[0] <= b[0] && souris[1] >= a[1] && souris[1] <= b[1];
                        self.cadre_saisi = Some(match proche {
                            // Le coin opposé reste en place.
                            Some(i) => {
                                let o = coins[3 - i];
                                Saisie::Tracer(((o[0] - coin[0]) / taille[0]) as f64, ((o[1] - coin[1]) / taille[1]) as f64)
                            }
                            None if dedans => Saisie::Deplacer(souris, x + l / 2.0, y + h / 2.0),
                            None => Saisie::Tracer(fx, fy),
                        });
                    }
                    let m = ui.io().mouse_wheel();
                    if m != 0.0 {
                        p.retenir();
                        let f = (z.facteur * if m > 0.0 { 1.08 } else { 1.0 / 1.08 }).clamp(noyau::montage::FACTEUR_MIN, noyau::montage::FACTEUR_MAX);
                        if let Some(zz) = p.montage.zooms.get_mut(indice) {
                            zz.facteur = noyau::arrondi::arrondi(f, 2);
                        }
                        p.modifie();
                    }
                }
                if !ui.is_mouse_down(MouseButton::Left) {
                    self.cadre_saisi = None;
                }
                if let (Some(saisie), Some(zz)) = (self.cadre_saisi, p.montage.zooms.get_mut(indice)) {
                    match saisie {
                        Saisie::Deplacer(depart, cx0, cy0) => {
                            zz.cx = etat::centre_borne(cx0 + ((souris[0] - depart[0]) / taille[0]) as f64, zz.facteur);
                            zz.cy = etat::centre_borne(cy0 + ((souris[1] - depart[1]) / taille[1]) as f64, zz.facteur);
                        }
                        Saisie::Tracer(ax, ay) => (zz.facteur, zz.cx, zz.cy) = etat::zoom_trace(ax, ay, fx, fy),
                    }
                    p.modifie();
                }
            }
        }
        // Le transport.
        let t = p.lecteur.position();
        {
            let _f = self.mono.map(|f| ui.push_font(f));
            ui.text_colored(charte::c(charte::CHIFFRE_SAISIE), temps(t));
            ui.same_line();
            ui.text_colored(charte::c(charte::UNIT), format!("/ {}", temps(p.montage.duree)));
        }
        ui.same_line();
        ui.dummy([10.0, 1.0]);
        ui.same_line();
        if bouton_forme(ui, "debut", Forme::Debut, false, "Au début") {
            p.lecteur.aller_a(0.0);
        }
        ui.same_line();
        if bouton_forme(ui, "reculer", Forme::Reculer, false, "5 s en arrière (J)") {
            p.lecteur.aller_a(t - 5.0);
        }
        ui.same_line();
        let en_lecture = p.lecteur.en_lecture();
        let forme = if en_lecture { Forme::Pause } else { Forme::Lire };
        if bouton_forme(ui, "lire", forme, true, if en_lecture { "Pause (Espace)" } else { "Lire (Espace)" }) {
            p.lecteur.basculer();
        }
        ui.same_line();
        if bouton_forme(ui, "stop", Forme::Stop, false, "Stop : revient d'où la lecture est partie") {
            p.lecteur.stop();
        }
        ui.same_line();
        if bouton_forme(ui, "avancer", Forme::Avancer, false, "5 s en avant (L)") {
            p.lecteur.aller_a(t + 5.0);
        }
        ui.same_line();
        if bouton_forme(ui, "fin", Forme::Fin, false, "À la fin") {
            p.lecteur.aller_a(p.montage.duree);
        }
        ui.same_line();
        ui.dummy([10.0, 1.0]);
        ui.same_line();
        ui.text_colored(charte::c(charte::UNIT), format!("montée {}", temps(p.montage.duree_montee())));
    }

    fn panneau_inspecteur(&mut self, ui: &Ui) {
        let Some(p) = &mut self.prise else { return };
        // Assez de place à droite des champs pour leur nom entier.
        let _largeur = ui.push_item_width(ui.content_region_avail()[0] - 150.0);
        let gras = self.gras;
        let titre = |ui: &Ui, t: &str| {
            let _f = gras.map(|f| ui.push_font(f));
            ui.text(t);
        };
        // Un champ de nombre : la valeur changée entre dans l'historique.
        let mut avant: Option<noyau::montage::Montage> = None;
        let nombre = |ui: &Ui, nom: &str, v: &mut f64, pas: f64| -> bool {
            let mut f = *v as f32;
            let change = ui.input_float(nom, &mut f);
            if change && (f as f64 - *v).abs() > 1e-6 {
                *v = noyau::arrondi::arrondi(f as f64, 3).max(0.0);
                let _ = pas;
                return true;
            }
            false
        };
        let copie = p.montage.clone();
        let mut change = false;
        match p.choix {
            Some(Choix { sorte: Sorte::Zoom, indice }) if indice < p.montage.zooms.len() => {
                titre(ui, &format!("Zoom n° {}", indice + 1));
                let z = &mut p.montage.zooms[indice];
                change |= nombre(ui, "Début (s)", &mut z.debut, 0.1);
                change |= nombre(ui, "Fin (s)", &mut z.fin, 0.1);
                change |= nombre(ui, "Grossissement", &mut z.facteur, 0.05);
                change |= nombre(ui, "Centre x (0-1)", &mut z.cx, 0.01);
                change |= nombre(ui, "Centre y (0-1)", &mut z.cy, 0.01);
                ui.text_colored(charte::c(charte::UNIT), format!("origine : {}  {}", z.origine, z.libelle));
                ui.text_wrapped("Dans la visionneuse, à l'arrêt : glisser dans le cadre le déplace, un coin le retaille, tracer à côté en fait un autre, la molette grossit.");
            }
            Some(Choix { sorte: Sorte::Etiquette, indice }) if indice < p.montage.etiquettes.len() => {
                titre(ui, "Étiquette");
                let e = &mut p.montage.etiquettes[indice];
                change |= ui.input_text("Texte", &mut e.texte).build();
                change |= nombre(ui, "Début (s)", &mut e.debut, 0.1);
                change |= nombre(ui, "Fin (s)", &mut e.fin, 0.1);
                let genres = ["reglage", "touche", "atelier", "fenetre"];
                let mut i = genres.iter().position(|g| *g == e.genre).unwrap_or(0);
                if ui.combo_simple_string("Genre", &mut i, &genres) {
                    e.genre = genres[i].into();
                    change = true;
                }
            }
            Some(Choix { sorte: Sorte::SousTitre, indice }) if indice < p.montage.sous_titres.len() => {
                titre(ui, "Sous-titre");
                let s = &mut p.montage.sous_titres[indice];
                change |= ui.input_text_multiline("##texte", &mut s.texte, [-1.0, 80.0]).build();
                change |= nombre(ui, "Début (s)", &mut s.debut, 0.1);
                change |= nombre(ui, "Fin (s)", &mut s.fin, 0.1);
            }
            Some(Choix { sorte: Sorte::Coupe, indice }) if indice < p.montage.coupes.len() => {
                titre(ui, "Coupe");
                let c = &mut p.montage.coupes[indice];
                change |= ui.checkbox("Active", &mut c.active);
                let vitesses = ["coupée", "×2", "×4", "×8"];
                let valeurs = [0.0, 2.0, 4.0, 8.0];
                let mut i = valeurs.iter().position(|v| *v == c.vitesse).unwrap_or(0);
                if ui.combo_simple_string("Passage", &mut i, &vitesses) {
                    c.vitesse = valeurs[i];
                    change = true;
                }
                change |= nombre(ui, "Début (s)", &mut c.debut, 0.1);
                change |= nombre(ui, "Fin (s)", &mut c.fin, 0.1);
                ui.text_colored(charte::c(charte::UNIT), format!("origine : {}", c.origine));
            }
            Some(Choix { sorte: Sorte::Clic, indice }) if indice < p.montage.clics.len() => {
                titre(ui, "Clic");
                let c = &mut p.montage.clics[indice];
                change |= nombre(ui, "Instant (s)", &mut c.t, 0.1);
            }
            _ => {
                titre(ui, "Réglages du montage");
                let r = &mut p.montage.reglages;
                change |= ui.checkbox("Montrer les clics", &mut r.montrer_clics);
                change |= ui.checkbox("Montrer les étiquettes", &mut r.montrer_etiquettes);
                change |= ui.checkbox("Mettre le réglage en lumière", &mut r.mettre_en_lumiere);
                change |= ui.checkbox("Montrer les sous-titres", &mut r.montrer_sous_titres);
                change |= ui.checkbox("Incruster les sous-titres dans l'image", &mut r.incruster_sous_titres);
                let mut cale = r.decalage_journal;
                if nombre(ui, "Calage des gestes (s)", &mut cale, 0.05) {
                    r.decalage_journal = cale;
                    change = true;
                }
                ui.separator();
                ui.text_wrapped("Un clic sur un élément de la ligne de temps le montre ici.");
            }
        }
        if change {
            avant = Some(copie);
        }
        if let Some(m) = avant {
            // L'état d'avant le changement entre dans l'historique.
            let apres = std::mem::replace(&mut p.montage, m);
            p.retenir();
            p.montage = apres;
            p.modifie();
        }
    }
}

impl Application for Monteur {
    fn configure_imgui(&mut self, context: &mut InitContext<'_>) -> Result<(), RunError> {
        let etape = ApplicationStage::ConfigureImgui;
        let police = |o| StbTrueTypeFontData::from_slice(o).map_err(|e| RunError::application(etape, e));
        let (inter, gras, mono) = (police(INTER)?, police(INTER_GRAS)?, police(MONO)?);
        let imgui = context.imgui();
        let atlas = imgui.font_atlas();
        atlas.add_font(&[FontSource::stb_truetype_with_size(inter, 15.0)]);
        self.gras = Some(atlas.add_font(&[FontSource::stb_truetype_with_size(gras, 15.0)]));
        self.mono = Some(atlas.add_font(&[FontSource::stb_truetype_with_size(mono, 15.0)]));
        charte::appliquer(imgui.style_mut());
        Ok(())
    }

    fn initialized(&mut self, _c: &mut InitializedContext<'_>, _g: &mut GpuContext<'_>) -> Result<(), RunError> {
        Ok(())
    }

    fn gpu_lost(&mut self, g: &mut GpuContext<'_>) -> Result<(), RunError> {
        self.vue.perdre(g.gpu());
        self.miniatures.liberer(g.gpu());
        Ok(())
    }

    fn frame(&mut self, context: &mut FrameContext<'_>) -> Result<(), RunError> {
        if let Some(mut s) = self.scenario.take() {
            let fin = s.avancer(self);
            self.scenario = Some(s);
            if fin {
                context.request_exit();
            }
        }
        if let Some(v) = self.a_ouvrir.take() {
            self.ouvrir(v);
        }
        if let Some(p) = &mut self.prise {
            if let Some(m) = p.suivre_le_fond() {
                self.message = m;
            }
            // En lecture, les passages coupés se sautent, comme au rendu.
            if p.lecteur.en_lecture() {
                let t = p.lecteur.position();
                if let Some((_, b)) = p.montage.coupes_actives().into_iter().find(|&(a, b)| a <= t && t < b - 0.05) {
                    p.lecteur.aller_a(b);
                }
            }
            if let Some(e) = p.ecrire_si_besoin(false) {
                self.message = e;
            }
        }
        let sans_zoom = self
            .prise
            .as_ref()
            .is_some_and(|p| matches!(p.choix, Some(Choix { sorte: Sorte::Zoom, .. })) && !p.lecteur.en_lecture());
        let image = match &mut self.prise {
            Some(p) => self.vue.preparer(context.gpu(), p, &self.polices, sans_zoom),
            None => None,
        };
        self.miniatures.recevoir(context.gpu());
        let textures = self.miniatures.textures(context.gpu());
        let ui = context.ui();
        self.touches(ui);

        let mut relire = false;
        if let Some(barre) = ui.begin_main_menu_bar() {
            if let Some(m) = ui.begin_menu("Fichier") {
                if ui.menu_item("Relire le dossier") {
                    let fiches = Fiches::lire(&chemin_fiches()).unwrap_or_default();
                    self.dossier = Dossier::lire(&self.dossier.chemin.clone(), &fiches);
                    relire = true;
                }
                m.end();
            }
            if let Some(m) = ui.begin_menu("Édition") {
                let (d, r) = self.prise.as_ref().map_or((false, false), |p| (p.peut_defaire(), p.peut_refaire()));
                if ui.menu_item_enabled_selected_with_shortcut("Annuler", "Ctrl+Z", false, d)
                    && let Some(p) = &mut self.prise
                {
                    p.defaire();
                }
                if ui.menu_item_enabled_selected_with_shortcut("Rétablir", "Ctrl+Maj+Z", false, r)
                    && let Some(p) = &mut self.prise
                {
                    p.refaire();
                }
                if ui.menu_item_with_shortcut("Supprimer l'élément", "Suppr")
                    && let Some(p) = &mut self.prise
                {
                    p.supprimer_choix();
                }
                m.end();
            }
            if let Some(m) = ui.begin_menu("Montage") {
                let pret = self.prise.as_ref().is_some_and(|p| p.preparation.is_none());
                if ui.menu_item_enabled_selected_no_shortcut("Préparer le montage", false, pret)
                    && let Some(p) = &mut self.prise
                {
                    p.preparer();
                    self.message = "Préparation en cours : mouvement de l'image, gestes, silences…".into();
                }
                m.end();
            }
            barre.end();
        }

        // La fenêtre hôte des panneaux, entre la barre de menus et la barre d'état.
        let vp = ui.main_viewport();
        ui.set_next_window_viewport(vp.id());
        let (pos, mut taille) = (vp.work_pos(), vp.work_size());
        ui.window("ruban")
            .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_DOCKING | WindowFlags::NO_SAVED_SETTINGS)
            .position(pos, Condition::Always)
            .size([taille[0], RUBAN], Condition::Always)
            .build(|| self.ruban(ui));
        let pos = [pos[0], pos[1] + RUBAN];
        taille[1] -= 24.0 + RUBAN;
        let arrondi = ui.push_style_var(StyleVar::WindowRounding(0.0));
        let bord = ui.push_style_var(StyleVar::WindowBorderSize(0.0));
        let marge = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
        let disposition = self.panneaux.disposition();
        let mut resultat = Ok(Id::default());
        ui.window("hote")
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_COLLAPSE
                    | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                    | WindowFlags::NO_NAV_FOCUS
                    | WindowFlags::NO_DOCKING,
            )
            .position(pos, Condition::Always)
            .size(taille, Condition::Always)
            .build(|| {
                let id = ui.get_id("panneaux");
                resultat = ui.dockspace().root_id(id).current_window(ui.content_region_avail()).layout(&disposition, DockLayoutApply::IfMissing).build();
            });
        marge.pop();
        bord.pop();
        arrondi.pop();
        resultat.map_err(|e| RunError::application(ApplicationStage::Frame, e))?;

        let panneaux = Panneaux::new();
        ui.window(&panneaux.prises).build(|| self.panneau_prises(ui, &textures));
        ui.window(&panneaux.visionneuse).build(|| self.panneau_visionneuse(ui, image));
        ui.window(&panneaux.inspecteur).build(|| self.panneau_inspecteur(ui));
        ui.window(&panneaux.ligne).build(|| {
            if let Some(p) = &mut self.prise {
                self.ligne.dessiner(ui, p);
            }
        });

        // La barre d'état.
        ui.window("etat")
            .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_DOCKING | WindowFlags::NO_SAVED_SETTINGS)
            .position([pos[0], pos[1] + taille[1]], Condition::Always)
            .size([taille[0], 24.0], Condition::Always)
            .build(|| {
                ui.text_colored(charte::c(charte::TEXT_2), &self.message);
                if let Some(p) = &self.prise {
                    if let Some(prep) = &p.preparation {
                        ui.same_line();
                        ui.text_colored(charte::c(charte::ACCENT_HOVER), format!("  préparation… {:.0} s", prep.debut.elapsed().as_secs_f64()));
                    }
                    ui.same_line_with_pos(taille[0] - 260.0);
                    ui.text_colored(charte::c(charte::UNIT), format!("image peinte en {:.1} ms · {:.0} i/s", self.vue.dernier_ms, ui.io().framerate()));
                }
            });
        if relire {
            self.miniatures.liberer(context.gpu());
            self.miniatures = miniatures::Miniatures::lancer(self.dossier.prises.iter().map(|p| p.0.clone()).collect());
        }
        Ok(())
    }

    fn shutdown(&mut self, _c: &mut dear_app::ShutdownContext<'_>) -> Result<(), RunError> {
        if let Some(p) = &mut self.prise
            && let Some(e) = p.ecrire_si_besoin(true)
        {
            eprintln!("{e}");
        }
        Ok(())
    }
}

/// Tout arrêt du programme s'écrit, avec l'endroit du code et la pile d'appels, dans
/// `~/.local/share/monteur/monteur2-plantages.log` (et sur la sortie d'erreur) : lancé depuis le
/// menu, rien d'autre n'en garderait trace.
fn noter_les_plantages() {
    let journal = chemin_fiches().with_file_name("monteur2-plantages.log");
    std::panic::set_hook(Box::new(move |info| {
        let pile = std::backtrace::Backtrace::force_capture();
        let texte = format!(
            "\n=== {} — Monteur 2 {} ===\n{info}\n{pile}\n",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            env!("CARGO_PKG_VERSION")
        );
        eprintln!("{texte}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&journal) {
            use std::io::Write;
            let _ = f.write_all(texte.as_bytes());
        }
    }));
}

fn main() {
    noter_les_plantages();
    // Un seul Monteur à la fois, le 1 ou le 2 (le même verrou).
    let _verrou = match noyau::verrou::Verrou::prendre(&chemin_fiches().with_file_name("monteur.verrou")) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let config = AppConfig {
        window_title: "Monteur 2".to_string(),
        window_size: (1640.0, 1000.0),
        present_mode: wgpu::PresentMode::Fifo,
        clear_color: charte::c(charte::BG_CANVAS),
        wgpu: WgpuConfig::from_preset(WgpuPreset::HighPerformance),
        docking: DockingConfig::application_managed(),
        ini_filename: None,
        redraw: RedrawMode::Poll,
        ..Default::default()
    };
    if let Err(e) = run(config, Monteur::new()) {
        let texte = format!("le Monteur 2 s'arrête : {e}");
        eprintln!("{texte}");
        let journal = chemin_fiches().with_file_name("monteur2-plantages.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(journal) {
            use std::io::Write;
            let _ = writeln!(f, "\n=== {texte}");
        }
        std::process::exit(1);
    }
}
