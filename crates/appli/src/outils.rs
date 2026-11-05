//! Les boutons du ruban et de la barre de montage, comme dans la maquette : une icône dessinée, son
//! nom dessous, rien au repos, un fond au survol. Les groupes se séparent d'un filet.

use dear_imgui_rs::{MouseButton, Ui};

pub const HAUT: f32 = 48.0;
const LARGE_MIN: f32 = 58.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Icone {
    Preparer,
    Rendre,
    Voix,
    Filmer,
    Verdier,
    Annuler,
    Retablir,
    Couper,
    Valider,
    Scinder,
    CouperAvant,
    CouperApres,
    Zoom,
    Chapitre,
    Intercaler,
    Dessus,
}

/// Comment il se montre.
#[derive(Clone, Copy, PartialEq)]
pub enum Allure {
    Simple,
    /// Fond orange : le geste principal.
    Primaire,
    /// Contour orange : un geste en cours, à finir.
    EnCours,
}

/// Un outil ; rend vrai s'il est cliqué.
pub fn outil(ui: &Ui, id: &str, icone: Icone, nom: &str, couleur: u32, allure: Allure, aide: &str) -> bool {
    let largeur = (ui.calc_text_size(nom)[0] + 16.0).max(LARGE_MIN);
    ui.invisible_button(id, [largeur, HAUT]);
    let (survol, tenu, clique) = (ui.is_item_hovered(), ui.is_item_active(), ui.is_item_clicked_with_button(MouseButton::Left));
    let (a, b) = (ui.item_rect_min(), ui.item_rect_max());
    let primaire = allure == Allure::Primaire;
    // Une seule liste de dessin à la fois : celle-ci se rend avant celle de l'icône.
    let dl = ui.get_window_draw_list();
    if primaire {
        let fond = if tenu { charte::ACCENT_PRESSED } else if survol { charte::ACCENT_HOVER } else { charte::ACCENT };
        dl.add_rect(a, b, charte::c(fond)).filled(true).rounding(6.0).build();
    } else if allure == Allure::EnCours {
        dl.add_rect(a, b, charte::vec(charte::ACCENT, 0.2)).filled(true).rounding(6.0).build();
        dl.add_rect(a, b, charte::c(charte::ACCENT)).rounding(6.0).thickness(1.5).build();
    } else if survol || tenu {
        dl.add_rect(a, b, charte::c(if tenu { charte::BG_PRESSED } else { charte::BG_HOVER })).filled(true).rounding(6.0).build();
    }
    drop(dl);
    let encre = if primaire { charte::ENCRE_SUR_ACCENT } else { couleur };
    dessiner(ui, icone, [(a[0] + b[0]) / 2.0, a[1] + 16.0], encre);
    let texte = if primaire { charte::ENCRE_SUR_ACCENT } else if survol { charte::TEXT } else { charte::TEXT_2 };
    let l = ui.calc_text_size(nom)[0];
    ui.get_window_draw_list().add_text([(a[0] + b[0] - l) / 2.0, a[1] + 28.0], charte::c(texte), nom);
    if survol && !aide.is_empty() {
        ui.tooltip_text(aide);
    }
    clique
}

/// Le filet entre deux groupes.
pub fn filet(ui: &Ui) {
    ui.same_line();
    let a = ui.cursor_screen_pos();
    ui.dummy([9.0, HAUT]);
    let x = a[0] + 4.0;
    ui.get_window_draw_list().add_line([x, a[1] + 6.0], [x, a[1] + HAUT - 6.0], charte::c(charte::BORDER)).thickness(1.0).build();
    ui.same_line();
}

fn dessiner(ui: &Ui, icone: Icone, c: [f32; 2], encre: u32) {
    let dl = ui.get_window_draw_list();
    let e = charte::c(encre);
    let p = |x: f32, y: f32| [c[0] + x, c[1] + y];
    let trait_ = |x0: f32, y0: f32, x1: f32, y1: f32| dl.add_line(p(x0, y0), p(x1, y1), e).thickness(1.6).build();
    let cadre = |x0: f32, y0: f32, x1: f32, y1: f32| dl.add_rect(p(x0, y0), p(x1, y1), e).rounding(1.5).thickness(1.5).build();
    let plein = |x0: f32, y0: f32, x1: f32, y1: f32| dl.add_rect(p(x0, y0), p(x1, y1), e).filled(true).rounding(1.5).build();
    let triangle = |a: [f32; 2], b: [f32; 2], d: [f32; 2]| dl.add_triangle(p(a[0], a[1]), p(b[0], b[1]), p(d[0], d[1]), e).filled(true).build();
    // Une flèche courbe, pour Annuler (s = 1) et Rétablir (s = −1).
    let boucle = |s: f32| {
        let mut avant = None;
        for i in 0..=14 {
            let t = (-0.15 + 3.6 * i as f32 / 14.0) as f32;
            let q = [s * 6.5 * t.cos(), 1.5 - 6.5 * t.sin()];
            if let Some(v) = avant {
                let v: [f32; 2] = v;
                trait_(v[0], v[1], q[0], q[1]);
            }
            avant = Some(q);
        }
        triangle([-s * 9.5, 0.5], [-s * 2.5, -2.0], [-s * 6.0, 6.5]);
    };
    match icone {
        Icone::Preparer => {
            // Une étincelle : le montage se fait seul.
            triangle([0.0, -9.0], [-3.0, 0.0], [3.0, 0.0]);
            triangle([0.0, 9.0], [-3.0, 0.0], [3.0, 0.0]);
            triangle([-9.0, 0.0], [0.0, -3.0], [0.0, 3.0]);
            triangle([9.0, 0.0], [0.0, -3.0], [0.0, 3.0]);
        }
        Icone::Rendre => {
            cadre(-9.0, -7.0, 9.0, 7.0);
            triangle([-3.0, -4.0], [-3.0, 4.0], [4.0, 0.0]);
        }
        Icone::Voix => {
            for (x, h) in [(-8.0, 2.0), (-4.0, 6.0), (0.0, 9.0), (4.0, 5.0), (8.0, 3.0)] {
                trait_(x, -h, x, h);
            }
        }
        Icone::Filmer => {
            dl.add_circle(p(0.0, 0.0), 8.0, e).thickness(1.6).build();
            dl.add_circle(p(0.0, 0.0), 4.0, e).filled(true).build();
        }
        Icone::Verdier => {
            cadre(-9.0, -7.0, 9.0, 7.0);
            trait_(-9.0, -3.5, 9.0, -3.5);
            dl.add_circle(p(0.0, 2.0), 2.5, e).filled(true).build();
        }
        Icone::Annuler => boucle(1.0),
        Icone::Retablir => boucle(-1.0),
        Icone::Couper => {
            dl.add_circle(p(-4.5, 5.5), 3.0, e).thickness(1.5).build();
            dl.add_circle(p(4.5, 5.5), 3.0, e).thickness(1.5).build();
            trait_(-3.0, 3.0, 6.0, -9.0);
            trait_(3.0, 3.0, -6.0, -9.0);
        }
        Icone::Valider => {
            trait_(-8.0, 1.0, -2.5, 6.5);
            trait_(-2.5, 6.5, 8.0, -6.0);
        }
        Icone::Scinder => {
            cadre(-10.0, -5.0, -2.5, 5.0);
            cadre(2.5, -5.0, 10.0, 5.0);
            trait_(0.0, -9.0, 0.0, 9.0);
        }
        Icone::CouperAvant => {
            trait_(4.0, -9.0, 4.0, 9.0);
            cadre(-10.0, -5.0, 0.5, 5.0);
            trait_(-10.0, 5.0, 0.5, -5.0);
            plein(6.5, -5.0, 10.0, 5.0);
        }
        Icone::CouperApres => {
            trait_(-4.0, -9.0, -4.0, 9.0);
            cadre(-0.5, -5.0, 10.0, 5.0);
            trait_(-0.5, 5.0, 10.0, -5.0);
            plein(-10.0, -5.0, -6.5, 5.0);
        }
        Icone::Zoom => {
            dl.add_circle(p(-2.0, -2.0), 6.5, e).thickness(1.6).build();
            trait_(3.0, 3.0, 8.5, 8.5);
            trait_(-5.0, -2.0, 1.0, -2.0);
            trait_(-2.0, -5.0, -2.0, 1.0);
        }
        Icone::Chapitre => {
            triangle([0.0, -8.0], [-7.0, 0.0], [7.0, 0.0]);
            triangle([0.0, 8.0], [-7.0, 0.0], [7.0, 0.0]);
        }
        Icone::Intercaler => {
            cadre(-10.0, -1.0, -3.5, 7.0);
            cadre(3.5, -1.0, 10.0, 7.0);
            plein(-2.0, 1.0, 2.0, 7.0);
            triangle([-4.0, -8.0], [4.0, -8.0], [0.0, -3.0]);
        }
        Icone::Dessus => {
            cadre(-9.0, -8.0, 4.0, 2.0);
            plein(-3.0, -2.0, 9.0, 8.0);
        }
    }
}
