//! La ligne de temps : la règle, une piste par sorte d'élément, la forme du son, la tête de
//! lecture. Cliquer dans la règle place la tête ; un élément se choisit d'un clic, se déplace en le
//! glissant, s'allonge par un bord ; glisser dans une piste vide en crée un, un double-clic en pose
//! un de 2 s. Molette : défiler ; Ctrl+molette : resserrer autour de la souris.

use dear_imgui_rs::{MouseButton, MouseCursor, Ui};
use noyau::montage::{Clic, Coupe, Etiquette, Montage, SousTitre, Zoom};

use crate::etat::{Choix, PICS_PAR_S, Prise, Sorte, temps};
use crate::miniatures::Bande;

const ENTETE: f32 = 120.0;
const REGLE: f32 = 22.0;
/// Un bord se saisit à tant de pixels.
const BORD: f32 = 6.0;

pub struct Ligne {
    /// La seconde au bord gauche, et combien de secondes tient un pixel.
    pub debut: f64,
    pub par_px: f64,
    /// La largeur de la prise la dernière fois : à l'ouverture d'une autre, tout se remontre.
    duree_vue: f64,
    prise_main: Option<Main>,
}

#[derive(Clone, Copy)]
enum Main {
    Tete,
    Deplacer { choix: Choix, depart_t: f64, a0: f64, b0: f64 },
    Bord { choix: Choix, gauche: bool },
    Creer { sorte: Sorte, t0: f64 },
}

struct Piste {
    nom: &'static str,
    sorte: Option<Sorte>,
    couleur: u32,
    hauteur: f32,
}

const PISTES: [Piste; 7] = [
    Piste { nom: "Vidéos", sorte: Some(Sorte::Insertion), couleur: charte::PISTE_VIDEO, hauteur: 46.0 },
    Piste { nom: "Zooms", sorte: Some(Sorte::Zoom), couleur: charte::PISTE_ZOOM, hauteur: 26.0 },
    Piste { nom: "Étiquettes", sorte: Some(Sorte::Etiquette), couleur: charte::PISTE_ETIQUETTE, hauteur: 26.0 },
    Piste { nom: "Clics", sorte: Some(Sorte::Clic), couleur: charte::TEXT_2, hauteur: 18.0 },
    Piste { nom: "Sous-titres", sorte: Some(Sorte::SousTitre), couleur: 0xBDBDBD, hauteur: 26.0 },
    Piste { nom: "Voix", sorte: None, couleur: charte::PISTE_VOIX, hauteur: 40.0 },
    Piste { nom: "Coupes", sorte: Some(Sorte::Coupe), couleur: charte::PISTE_COUPE, hauteur: 24.0 },
];

/// Début et fin d'un élément (un clic : son instant, sur un dixième de seconde).
pub fn intervalle(m: &Montage, c: Choix) -> Option<(f64, f64)> {
    let i = c.indice;
    match c.sorte {
        Sorte::Zoom => m.zooms.get(i).map(|e| (e.debut, e.fin)),
        Sorte::Etiquette => m.etiquettes.get(i).map(|e| (e.debut, e.fin)),
        Sorte::Clic => m.clics.get(i).map(|e| (e.t, e.t + 0.1)),
        Sorte::SousTitre => m.sous_titres.get(i).map(|e| (e.debut, e.fin)),
        Sorte::Coupe => m.coupes.get(i).map(|e| (e.debut, e.fin)),
        // Intercalée, elle ne prend pas de temps sur la prise : une marque à son instant.
        Sorte::Insertion => m.insertions.get(i).map(|e| (e.t, e.t + if e.par_dessus() { e.duree() } else { 0.1 })),
    }
}

fn regler(m: &mut Montage, c: Choix, a: f64, b: f64) {
    let (a, b) = (noyau::arrondi::arrondi(a, 3), noyau::arrondi::arrondi(b, 3));
    let i = c.indice;
    match c.sorte {
        Sorte::Zoom => m.zooms.get_mut(i).map(|e| (e.debut, e.fin) = (a, b)),
        Sorte::Etiquette => m.etiquettes.get_mut(i).map(|e| (e.debut, e.fin) = (a, b)),
        Sorte::Clic => m.clics.get_mut(i).map(|e| e.t = a),
        Sorte::SousTitre => m.sous_titres.get_mut(i).map(|e| (e.debut, e.fin) = (a, b)),
        Sorte::Coupe => m.coupes.get_mut(i).map(|e| (e.debut, e.fin) = (a, b)),
        Sorte::Insertion => m.insertions.get_mut(i).map(|e| {
            e.t = a;
            if e.par_dessus() {
                e.fin = noyau::arrondi::arrondi(e.debut + (b - a), 3);
            }
        }),
    };
}

fn nombre(m: &Montage, s: Sorte) -> usize {
    match s {
        Sorte::Zoom => m.zooms.len(),
        Sorte::Etiquette => m.etiquettes.len(),
        Sorte::Clic => m.clics.len(),
        Sorte::SousTitre => m.sous_titres.len(),
        Sorte::Coupe => m.coupes.len(),
        Sorte::Insertion => m.insertions.len(),
    }
}

fn libelle(m: &Montage, c: Choix) -> String {
    let i = c.indice;
    match c.sorte {
        Sorte::Zoom => m.zooms.get(i).map(|z| format!("×{:.1}", z.facteur).replace('.', ",")),
        Sorte::Etiquette => m.etiquettes.get(i).map(|e| e.texte.clone()),
        Sorte::Clic => None,
        Sorte::Insertion => m.insertions.get(i).map(|e| {
            let nom = e.video.rsplit('/').next().unwrap_or(&e.video);
            format!("{nom} ({})", temps(e.duree()))
        }),
        Sorte::SousTitre => m.sous_titres.get(i).map(|s| s.texte.clone()),
        Sorte::Coupe => m.coupes.get(i).map(|c| {
            if !c.active {
                "proposée".into()
            } else if c.vitesse > 1.0 {
                format!("×{}", c.vitesse)
            } else {
                String::new()
            }
        }),
    }
    .unwrap_or_default()
}

/// Un élément neuf de la sorte voulue, de `a` à `b`.
fn creer(m: &mut Montage, sorte: Sorte, a: f64, b: f64) -> Choix {
    let (a, b) = (noyau::arrondi::arrondi(a, 3), noyau::arrondi::arrondi(b, 3));
    let autres = Default::default;
    match sorte {
        Sorte::Zoom => m.zooms.push(Zoom::nouveau(a, b)),
        Sorte::Etiquette => m.etiquettes.push(Etiquette {
            debut: a,
            fin: b,
            texte: "Étiquette".into(),
            x: 0.5,
            y: 0.5,
            genre: "reglage".into(),
            origine: "main".into(),
            rect: None,
            autres: autres(),
        }),
        Sorte::Clic => m.clics.push(Clic { t: a, x: 0.5, y: 0.5, bouton: "gauche".into(), autres: autres() }),
        Sorte::SousTitre => m.sous_titres.push(SousTitre { debut: a, fin: b, texte: String::new(), autres: autres() }),
        Sorte::Coupe => m.coupes.push(Coupe { debut: a, fin: b, active: true, origine: "main".into(), vitesse: 0.0, autres: autres() }),
        // Une vidéo insérée ne se crée pas à la souris : il faut choisir son fichier.
        Sorte::Insertion => {}
    }
    let indice = nombre(m, sorte) - 1;
    Choix { sorte, indice }
}

/// Après un tri, l'élément choisi a pu changer de place : on le retrouve par son début.
fn retrouver(m: &mut Montage, c: Choix) -> Choix {
    let Some((a, b)) = intervalle(m, c) else { return c };
    m.trier();
    let indice = (0..nombre(m, c.sorte))
        .find(|&i| intervalle(m, Choix { sorte: c.sorte, indice: i }) == Some((a, b)))
        .unwrap_or(c.indice);
    Choix { sorte: c.sorte, indice }
}

/// Des hachures « / » dans un rectangle, tous les 10 pixels (fixes à l'écran : elles ne glissent
/// pas quand le rectangle grandit).
fn hachurer(dl: &dear_imgui_rs::DrawListMut<'_>, x0: f32, y0: f32, x1: f32, y1: f32, couleur: [f32; 4]) {
    let h = y1 - y0;
    let mut s = ((x0 - h) / 10.0).floor() * 10.0;
    while s < x1 {
        // Le trait va de (s, y1) à (s + h, y0), borné au rectangle.
        let (mut ax, mut ay, mut bx, mut by) = (s, y1, s + h, y0);
        if ax < x0 {
            ay -= x0 - ax;
            ax = x0;
        }
        if bx > x1 {
            by += bx - x1;
            bx = x1;
        }
        if ax < bx {
            dl.add_line([ax, ay], [bx, by], couleur).build();
        }
        s += 10.0;
    }
}

impl Ligne {
    pub fn new() -> Self {
        Self { debut: 0.0, par_px: 0.05, duree_vue: -1.0, prise_main: None }
    }

    fn x_de(&self, t: f64, x0: f32) -> f32 {
        x0 + ((t - self.debut) / self.par_px) as f32
    }

    fn t_de(&self, x: f32, x0: f32) -> f64 {
        self.debut + (x - x0) as f64 * self.par_px
    }

    pub fn dessiner(&mut self, ui: &Ui, prise: &mut Prise, bande: (&Bande, Option<dear_imgui_rs::TextureId>)) {
        let dispo = ui.content_region_avail();
        let origine = ui.cursor_screen_pos();
        let largeur = (dispo[0] - ENTETE).max(50.0);
        let (x0, x1) = (origine[0] + ENTETE, origine[0] + ENTETE + largeur);
        let duree = prise.montage.duree.max(0.1);
        if self.duree_vue != duree {
            // Une autre prise : on la montre entière.
            self.duree_vue = duree;
            self.debut = 0.0;
            self.par_px = duree / largeur as f64;
        }
        let tete = prise.lecteur.position();
        // En lecture, la tête reste en vue.
        if prise.lecteur.en_lecture() {
            let fin_vue = self.t_de(x1, x0);
            if tete > fin_vue || tete < self.debut {
                self.debut = (tete - 0.1 * largeur as f64 * self.par_px).max(0.0);
            }
        }
        let dl = ui.get_window_draw_list();
        let c = charte::c;

        // La règle : une graduation lisible selon le resserrement.
        dl.add_rect(origine, [x1, origine[1] + REGLE], c(charte::BG_TITLE)).filled(true).build();
        let pas = [0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0]
            .into_iter()
            .find(|p| p / self.par_px >= 70.0)
            .unwrap_or(1200.0);
        let mut g = (self.debut / pas).floor() * pas;
        while self.x_de(g, x0) < x1 {
            let x = self.x_de(g, x0);
            if x >= x0 {
                dl.add_line([x, origine[1] + 12.0], [x, origine[1] + REGLE], c(charte::BORDER)).build();
                dl.add_text([x + 3.0, origine[1] + 3.0], c(charte::UNIT), temps(g));
            }
            g += pas;
        }

        // Les pistes.
        // Les pistes remplissent le panneau : le tirer vers le haut les agrandit (jusqu'au double).
        let total: f32 = PISTES.iter().map(|p| p.hauteur).sum();
        let k = ((dispo[1] - REGLE) / total).clamp(1.0, 2.0);
        let mut y = origine[1] + REGLE;
        let mut rangees = Vec::new();
        for piste in &PISTES {
            let h = piste.hauteur * k;
            dl.add_rect([x0, y], [x1, y + h - 1.0], [0.102, 0.102, 0.102, 1.0]).filled(true).build();
            dl.add_rect([origine[0] + 8.0, y + h / 2.0 - 4.0], [origine[0] + 16.0, y + h / 2.0 + 4.0], c(piste.couleur))
                .filled(true)
                .rounding(2.0)
                .build();
            dl.add_text([origine[0] + 22.0, y + h / 2.0 - 8.0], c(charte::TEXT), piste.nom);
            rangees.push((y, h, piste));
            y += h;
        }
        let bas = y;

        // La barre de la prise, sur la piste Vidéos : ses images, du début à la fin.
        if let Some(&(yb, hb, _)) = rangees.iter().find(|r| r.2.sorte == Some(Sorte::Insertion)) {
            let (ax, bx) = (self.x_de(0.0, x0).max(x0), self.x_de(duree, x0).min(x1));
            dl.add_rect([ax, yb + 2.0], [bx, yb + hb - 3.0], c(charte::BG_CARD)).filled(true).rounding(3.0).build();
            if let (Some(id), true) = (bande.1, bande.0.nombre > 0) {
                let b = bande.0;
                let hauteur = hb - 5.0;
                let largeur_image = hauteur * 1.6;
                let mut x = ax;
                while x < bx {
                    let t = self.t_de(x + largeur_image / 2.0, x0).clamp(0.0, duree);
                    let n = ((t / duree * b.nombre as f64) as u32).min(b.nombre - 1);
                    let (col, rang) = ((n % b.colonnes) as f32, (n / b.colonnes) as f32);
                    let (cl, rl) = (b.colonnes as f32, b.rangees as f32);
                    let vu = ((bx - x) / largeur_image).min(1.0);
                    dl.add_image(
                        id,
                        [x, yb + 2.0],
                        [x + largeur_image * vu, yb + 2.0 + hauteur],
                        [col / cl, rang / rl],
                        [(col + vu) / cl, (rang + 1.0) / rl],
                        [1.0, 1.0, 1.0, 0.8],
                    );
                    x += largeur_image;
                }
            }
            dl.add_rect([ax, yb + 2.0], [bx, yb + hb - 3.0], c(charte::TEXT_2)).rounding(3.0).build();
        }

        // Les passages coupés se voient sur toutes les pistes : un voile et des hachures ; un passage
        // accéléré, juste un voile léger (il reste dans la vidéo).
        let haut_pistes = origine[1] + REGLE;
        for coupe in prise.montage.coupes.iter().filter(|c| c.active) {
            let (xa, xb) = (self.x_de(coupe.debut, x0).max(x0), self.x_de(coupe.fin, x0).min(x1));
            if xb <= xa {
                continue;
            }
            if coupe.vitesse > 1.0 {
                dl.add_rect([xa, haut_pistes], [xb, bas], charte::vec(charte::ALERTE, 0.08)).filled(true).build();
            } else {
                dl.add_rect([xa, haut_pistes], [xb, bas], charte::vec(charte::PISTE_COUPE, 0.18)).filled(true).build();
                hachurer(&dl, xa, haut_pistes, xb, bas, charte::vec(charte::PISTE_COUPE, 0.35));
            }
        }

        // La forme du son.
        if let Some(&(yv, hv, _)) = rangees.iter().find(|r| r.2.sorte.is_none()) {
            let pics = &prise.pics;
            // À l'échelle du plus fort de la prise : une voix monte rarement haut.
            let plus_fort = pics.iter().copied().fold(0.05f32, f32::max);
            let milieu = yv + hv / 2.0;
            let mut x = x0;
            while x < x1 && !pics.is_empty() {
                let (ta, tb) = (self.t_de(x, x0), self.t_de(x + 2.0, x0));
                let (ia, ib) = ((ta * PICS_PAR_S as f64) as usize, ((tb * PICS_PAR_S as f64) as usize).max((ta * PICS_PAR_S as f64) as usize + 1));
                if ia >= pics.len() {
                    break;
                }
                let p = pics[ia..ib.min(pics.len())].iter().copied().fold(0.0f32, f32::max);
                let demi = (p / plus_fort * (hv - 6.0) / 2.0).max(0.5);
                let coupe = prise.montage.dans_une_coupe(ta);
                let couleur = if coupe { charte::vec(charte::PISTE_VOIX, 0.3) } else { charte::vec(charte::PISTE_VOIX, 0.85) };
                dl.add_line([x, milieu - demi], [x, milieu + demi], couleur).build();
                x += 2.0;
            }
        }

        // Les éléments.
        let souris = ui.io().mouse_pos();
        let mut sous_souris: Option<(Choix, bool, bool)> = None; // choix, bord gauche, bord droit
        for &(yp, hp, piste) in &rangees {
            let Some(sorte) = piste.sorte else { continue };
            for indice in 0..nombre(&prise.montage, sorte) {
                let ch = Choix { sorte, indice };
                let Some((a, b)) = intervalle(&prise.montage, ch) else { continue };
                let (xa, mut xb) = (self.x_de(a, x0), self.x_de(b, x0));
                // Intercalée, elle n'occupe pas de temps sur la prise : une marque assez large pour
                // la saisir.
                let ponctuel = sorte == Sorte::Clic || (sorte == Sorte::Insertion && !prise.montage.insertions[indice].par_dessus());
                if sorte == Sorte::Clic {
                    xb = xa + 3.0;
                } else if ponctuel {
                    xb = xa + 90.0;
                }
                if xb < x0 || xa > x1 {
                    continue;
                }
                let (xa, xb) = (xa.max(x0), xb.min(x1).max(xa.max(x0) + 2.0));
                let choisi = prise.choix == Some(ch);
                let proposee = sorte == Sorte::Coupe && !prise.montage.coupes[indice].active;
                let couleur = if sorte == Sorte::Coupe && prise.montage.coupes[indice].vitesse > 1.0 { charte::ALERTE } else { piste.couleur };
                let fond: f32 = if choisi { 0.85 } else if proposee { 0.08 } else { 0.25 };
                let fond = if sorte == Sorte::Insertion { fond.max(0.75) } else { fond };
                dl.add_rect([xa, yp + 3.0], [xb, yp + hp - 4.0], charte::vec(couleur, fond)).filled(true).rounding(3.0).build();
                dl.add_rect([xa, yp + 3.0], [xb, yp + hp - 4.0], c(couleur)).rounding(3.0).thickness(if choisi { 2.0 } else { 1.0 }).build();
                let texte = libelle(&prise.montage, ch);
                if !texte.is_empty() && xb - xa > 24.0 {
                    let visible: String = texte.chars().take(((xb - xa - 8.0) / 7.0) as usize).collect();
                    let encre = if choisi || sorte == Sorte::Insertion { charte::ENCRE_SUR_ACCENT } else { charte::TEXT };
                    dl.add_text([xa + 4.0, yp + hp / 2.0 - 8.0], c(encre), visible);
                }
                if souris[1] >= yp && souris[1] < yp + hp && souris[0] >= xa - BORD && souris[0] <= xb + BORD {
                    let gauche = !ponctuel && (souris[0] - xa).abs() <= BORD;
                    let droit = !ponctuel && (souris[0] - xb).abs() <= BORD;
                    if souris[0] >= xa - BORD && souris[0] <= xb + BORD {
                        sous_souris = Some((ch, gauche, droit));
                    }
                }
            }
        }

        // Le début d'une coupe posée par C.
        // Elle grandit jusqu'à la tête de lecture, dans un sens ou dans l'autre, comme au Monteur 1.
        if let Some(a) = prise.coupe_commencee {
            let (d, f) = (a.min(tete), a.max(tete));
            let (xa, xb) = (self.x_de(d, x0).max(x0), self.x_de(f, x0).min(x1).max(self.x_de(d, x0).max(x0) + 2.0));
            dl.add_rect([xa, haut_pistes], [xb, bas], charte::vec(charte::PISTE_COUPE, 0.24)).filled(true).build();
            hachurer(&dl, xa, haut_pistes, xb, bas, charte::vec(charte::PISTE_COUPE, 0.6));
            let xd = self.x_de(a, x0);
            dl.add_line([xd, origine[1]], [xd, bas], c(charte::PISTE_COUPE)).thickness(2.0).build();
        }

        // La souris.
        ui.set_cursor_screen_pos([x0, origine[1]]);
        ui.invisible_button("ligne", [largeur, (bas - origine[1]).max(1.0)]);
        let survol = ui.is_item_hovered();
        let t_souris = self.t_de(souris[0], x0).clamp(0.0, duree);
        let piste_souris = rangees.iter().find(|r| souris[1] >= r.0 && souris[1] < r.0 + r.1).and_then(|r| r.2.sorte);
        if survol && let Some((_, g, d)) = sous_souris
            && (g || d)
        {
            ui.set_mouse_cursor(Some(MouseCursor::ResizeEW));
        }
        if survol && ui.is_mouse_clicked(MouseButton::Left) {
            self.prise_main = if souris[1] < origine[1] + REGLE {
                Some(Main::Tete)
            } else if let Some((ch, g, d)) = sous_souris {
                prise.choix = Some(ch);
                let (a0, b0) = intervalle(&prise.montage, ch).unwrap_or((0.0, 0.0));
                // Un zoom choisi se règle sur son image : la tête y vient si elle est ailleurs.
                if ch.sorte == Sorte::Zoom && !prise.lecteur.en_lecture() && !(a0..=b0).contains(&tete) {
                    prise.lecteur.aller_a((a0 + b0) / 2.0);
                }
                prise.retenir();
                Some(if g || d { Main::Bord { choix: ch, gauche: g } } else { Main::Deplacer { choix: ch, depart_t: t_souris, a0, b0 } })
            } else {
                prise.choix = None;
                piste_souris.filter(|s| *s != Sorte::Insertion).map(|sorte| Main::Creer { sorte, t0: t_souris }).or(Some(Main::Tete))
            };
        }
        if survol && ui.is_mouse_double_clicked(MouseButton::Left) && sous_souris.is_none()
            && let Some(sorte) = piste_souris.filter(|s| *s != Sorte::Insertion)
        {
            prise.retenir();
            let ch = creer(&mut prise.montage, sorte, t_souris, (t_souris + 2.0).min(duree));
            prise.choix = Some(retrouver(&mut prise.montage, ch));
            prise.modifie();
            self.prise_main = None;
        }
        if ui.is_mouse_down(MouseButton::Left) {
            match self.prise_main {
                Some(Main::Tete) => prise.lecteur.aller_a(t_souris),
                Some(Main::Deplacer { choix, depart_t, a0, b0 }) => {
                    // Un élément plus long que la vidéo (une fin tapée à la main) : `clamp` refuse un
                    // plancher au-dessus du plafond et arrêterait tout.
                    let plancher = -a0;
                    let d = (t_souris - depart_t).clamp(plancher, (duree - b0).max(plancher));
                    regler(&mut prise.montage, choix, a0 + d, b0 + d);
                    prise.modifie();
                }
                Some(Main::Bord { choix, gauche }) => {
                    if let Some((a, b)) = intervalle(&prise.montage, choix) {
                        let (a, b) = if gauche { (t_souris.min(b - 0.1), b) } else { (a, t_souris.max(a + 0.1)) };
                        regler(&mut prise.montage, choix, a, b);
                        prise.modifie();
                    }
                }
                Some(Main::Creer { t0, .. }) => {
                    let (a, b) = (t0.min(t_souris), t0.max(t_souris));
                    let xa = self.x_de(a, x0);
                    let xb = self.x_de(b, x0);
                    if xb - xa > 4.0 {
                        dl.add_rect([xa, origine[1] + REGLE], [xb, bas], charte::vec(charte::ACCENT, 0.15)).filled(true).build();
                    }
                }
                None => {}
            }
        }
        if ui.is_mouse_released(MouseButton::Left) {
            match self.prise_main.take() {
                Some(Main::Creer { sorte, t0 }) => {
                    let (a, b) = (t0.min(t_souris), t0.max(t_souris));
                    if (b - a) / self.par_px > 4.0 {
                        prise.retenir();
                        let ch = creer(&mut prise.montage, sorte, a, b);
                        prise.choix = Some(retrouver(&mut prise.montage, ch));
                        prise.modifie();
                    } else {
                        prise.lecteur.aller_a(t_souris);
                    }
                }
                Some(Main::Deplacer { choix, .. }) | Some(Main::Bord { choix, .. }) => {
                    prise.choix = Some(retrouver(&mut prise.montage, choix));
                }
                _ => {}
            }
        }
        // Molette : défiler ; Ctrl : resserrer autour de la souris.
        let molette = ui.io().mouse_wheel();
        if survol && molette != 0.0 {
            if ui.io().key_ctrl() {
                let avant = t_souris;
                self.par_px = (self.par_px * if molette > 0.0 { 0.8 } else { 1.25 }).clamp(0.002, duree / 200.0 + 0.002);
                self.debut = avant - (souris[0] - x0) as f64 * self.par_px;
            } else {
                self.debut -= molette as f64 * 60.0 * self.par_px;
            }
        }
        self.debut = self.debut.clamp(0.0, (duree - largeur as f64 * self.par_px).max(0.0));

        // La tête de lecture.
        let xt = self.x_de(tete, x0);
        if xt >= x0 && xt <= x1 {
            dl.add_line([xt, origine[1]], [xt, bas], c(charte::ACCENT)).thickness(2.0).build();
            dl.add_triangle([xt - 6.0, origine[1]], [xt + 6.0, origine[1]], [xt, origine[1] + 10.0], c(charte::ACCENT))
                .filled(true)
                .build();
        }
        if survol && let Some((ch, _, _)) = sous_souris {
            if let Some((a, b)) = intervalle(&prise.montage, ch) {
                ui.tooltip_text(format!("{} → {}  {}", temps(a), temps(b), libelle(&prise.montage, ch)));
            }
        }
    }
}
