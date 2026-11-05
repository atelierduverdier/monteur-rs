//! La fenêtre « Nettoyer la voix » : réglages valables pour toutes les prises, et écoute avant/après
//! des 10 secondes autour de la tête de lecture.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use dear_imgui_rs::{Condition, Ui};
use noyau::voix::Voix;

const DUREE: f64 = 10.0;
const COLONNES: usize = 260;

struct Son {
    pcm: Vec<u8>,
    pics: Vec<f32>,
    lufs: Option<f64>,
}

fn son(e: rendu::Extrait) -> Son {
    let par_colonne = (e.pcm.len() / 4 / COLONNES).max(1);
    let pics = e
        .pcm
        .chunks(par_colonne * 4)
        .map(|c| c.chunks_exact(2).map(|o| i16::from_le_bytes([o[0], o[1]]).unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0)
        .collect();
    Son { pcm: e.pcm, pics, lufs: e.lufs }
}

pub struct FenetreVoix {
    pub ouverte: bool,
    pub reglages: Voix,
    /// Ce qu'Annuler remet.
    gardes: Voix,
    sons: Option<(Son, Son)>,
    calcul: Option<Receiver<Option<(Son, Son)>>>,
    a_refaire: Option<Instant>,
    ecoute: Option<Child>,
}

fn arreter(ecoute: &mut Option<Child>) {
    if let Some(mut c) = ecoute.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

impl Drop for FenetreVoix {
    fn drop(&mut self) {
        arreter(&mut self.ecoute);
    }
}

fn ecouter(pcm: &[u8]) -> Option<Child> {
    let mut enfant = Command::new("paplay")
        .args(["--raw", "--format=s16le", "--rate=48000", "--channels=2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut entree = enfant.stdin.take()?;
    let copie = pcm.to_vec();
    std::thread::spawn(move || {
        let _ = entree.write_all(&copie);
    });
    Some(enfant)
}

fn onde(ui: &Ui, nom: &str, pics: Option<&[f32]>, couleur: u32) {
    ui.align_text_to_frame_padding();
    ui.text_colored(charte::c(charte::TEXT_2), nom);
    ui.same_line_with_pos(60.0);
    let a = ui.cursor_screen_pos();
    let l = ui.content_region_avail()[0];
    const H: f32 = 44.0;
    ui.dummy([l, H]);
    let dl = ui.get_window_draw_list();
    dl.add_rect(a, [a[0] + l, a[1] + H], charte::c(charte::BG_INPUT)).filled(true).build();
    let Some(pics) = pics else { return };
    let pas = l / pics.len().max(1) as f32;
    for (i, p) in pics.iter().enumerate() {
        let x = a[0] + i as f32 * pas;
        let d = (p * H / 2.0).max(0.5);
        dl.add_line([x, a[1] + H / 2.0 - d], [x, a[1] + H / 2.0 + d], charte::c(couleur)).thickness(pas.max(1.0) * 0.8).build();
    }
}

fn curseur(ui: &Ui, nom: &str, v: &mut f64, min: f32, max: f32, format: &str) -> bool {
    let mut x = *v as f32;
    let change = match dear_imgui_rs::NumericFormat::<f32>::new(format.to_string()) {
        Ok(f) => ui.slider_config(nom, min, max).display_format(f).build(&mut x),
        Err(_) => ui.slider_f32(nom, &mut x, min, max),
    };
    if change {
        *v = x as f64;
    }
    change
}

/// Une carte : sa case, une ligne d'explication, ses curseurs.
fn carte(ui: &Ui, l: f32, id: &str, nom: &str, explication: &str, active: &mut bool, reglages: impl FnOnce(&Ui) -> bool) -> bool {
    let mut change = false;
    ui.child_window(id).size([l, 128.0]).border(true).build(ui, || {
        change |= ui.checkbox(nom, active);
        ui.text_colored(charte::c(charte::TEXT_2), explication);
        ui.spacing();
        ui.set_next_item_width(l - 120.0);
        change |= reglages(ui);
    });
    change
}

impl FenetreVoix {
    pub fn new(chemin: &Path) -> Self {
        let reglages = Voix::lire(chemin);
        Self { ouverte: false, gardes: reglages.clone(), reglages, sons: None, calcul: None, a_refaire: None, ecoute: None }
    }

    pub fn ouvrir(&mut self) {
        self.ouverte = true;
        self.sons = None;
        self.a_refaire = Some(Instant::now());
    }

    fn calculer(&mut self, video: PathBuf, t: f64) {
        let voix = self.reglages.clone();
        let debut = (t - DUREE / 2.0).max(0.0);
        let (envoi, recu) = mpsc::channel();
        std::thread::spawn(move || {
            let r = rendu::avant_apres(&video, debut, DUREE, &voix).map(|(a, b)| (son(a), son(b)));
            let _ = envoi.send(r);
        });
        self.calcul = Some(recu);
    }

    /// `true` : une écoute démarre, la lecture de la prise doit se taire.
    pub fn dessiner(&mut self, ui: &Ui, video: Option<&Path>, t: f64, chemin: &Path, message: &mut String) -> bool {
        if !self.ouverte {
            arreter(&mut self.ecoute);
            return false;
        }
        if let Some(r) = &self.calcul
            && let Ok(s) = r.try_recv()
        {
            self.calcul = None;
            self.sons = s;
        }
        if self.calcul.is_none()
            && self.a_refaire.is_some_and(|i| i.elapsed() > Duration::from_millis(350))
            && let Some(v) = video
        {
            self.a_refaire = None;
            self.calculer(v.to_path_buf(), t);
        }
        let mut ouverte = true;
        let mut change = false;
        let mut lance = false;
        ui.window("Nettoyer la voix###voix").opened(&mut ouverte).size([920.0, 660.0], Condition::FirstUseEver).build(|| {
            ui.text_colored(charte::c(charte::TEXT_2), "Les réglages se gardent pour toutes les prises.");
            ui.spacing();
            onde(ui, "Avant", self.sons.as_ref().map(|s| s.0.pics.as_slice()), charte::TEXT_DISABLED);
            onde(ui, "Après", self.sons.as_ref().map(|s| s.1.pics.as_slice()), charte::SUCCES);
            ui.spacing();
            if ui.button_with_size("Écouter après", [130.0, 30.0])
                && let Some(s) = &self.sons
            {
                arreter(&mut self.ecoute);
                self.ecoute = ecouter(&s.1.pcm);
                lance = true;
            }
            ui.same_line();
            if ui.button_with_size("Écouter avant", [130.0, 30.0])
                && let Some(s) = &self.sons
            {
                arreter(&mut self.ecoute);
                self.ecoute = ecouter(&s.0.pcm);
                lance = true;
            }
            ui.same_line();
            if ui.button_with_size("Stop##voix", [70.0, 30.0]) {
                arreter(&mut self.ecoute);
            }
            ui.same_line();
            ui.align_text_to_frame_padding();
            let etat = if self.calcul.is_some() || self.a_refaire.is_some() {
                "calcul…".to_string()
            } else if video.is_none() {
                "ouvre une prise pour écouter".to_string()
            } else {
                match &self.sons {
                    Some((a, b)) => format!(
                        "10 s autour de la tête · {} → {} LUFS",
                        a.lufs.map_or("?".to_string(), |l| format!("{l:.1}")),
                        b.lufs.map_or("?".to_string(), |l| format!("{l:.1}"))
                    ),
                    None => "écoute impossible sur ce passage".to_string(),
                }
            };
            ui.text_colored(charte::c(charte::TEXT_2), etat);
            ui.spacing();
            // Une seule largeur : calculée à chaque carte, la seconde colonne sortait plus étroite.
            let l = (ui.content_region_avail()[0] - 8.0) / 2.0;
            let r = &mut self.reglages;
            change |= carte(ui, l, "c1", "Réduire le bruit", "Souffle du PC et ventilation.", &mut r.bruit, |ui| {
                let mut p = r.bruit_force * 100.0;
                let c = curseur(ui, "Force##bruit", &mut p, 0.0, 100.0, "%.0f %%");
                r.bruit_force = p / 100.0;
                c
            });
            ui.same_line();
            change |= carte(ui, l, "c2", "Couper le grave", "Chocs sur le bureau, ronflement.", &mut r.grave, |ui| curseur(ui, "Fréquence", &mut r.grave_hz, 40.0, 200.0, "%.0f Hz"));
            change |= carte(ui, l, "c3", "Égaliser la voix", "Rapproche passages forts et faibles.", &mut r.egaliser, |ui| {
                let c = curseur(ui, "Rapport", &mut r.rapport, 1.5, 8.0, "%.1f:1");
                ui.set_next_item_width(ui.content_region_avail()[0] - 110.0);
                c | curseur(ui, "Seuil", &mut r.seuil_db, -40.0, -6.0, "%.0f dB")
            });
            ui.same_line();
            change |= carte(ui, l, "c4", "Adoucir les « s »", "Atténue les sifflantes trop vives.", &mut r.adoucir, |ui| {
                let mut p = r.adoucir_force * 100.0;
                let c = curseur(ui, "Force##s", &mut p, 0.0, 100.0, "%.0f %%");
                r.adoucir_force = p / 100.0;
                c
            });
            change |= carte(ui, l, "c5", "Niveau YouTube", "Même volume partout, sans saturer.", &mut r.niveau, |ui| {
                let c = curseur(ui, "Niveau visé", &mut r.niveau_lufs, -24.0, -10.0, "%.0f LUFS");
                ui.set_next_item_width(ui.content_region_avail()[0] - 110.0);
                c | curseur(ui, "Crête max", &mut r.crete_db, -3.0, 0.0, "%.1f dB")
            });
            ui.spacing();
            if ui.button_with_size("Annuler##voix", [100.0, 30.0]) {
                self.reglages = self.gardes.clone();
                self.ouverte = false;
            }
            ui.same_line();
            if ui.button_with_size("Garder ces réglages", [180.0, 30.0]) {
                match self.reglages.ecrire(chemin) {
                    Ok(()) => {
                        self.gardes = self.reglages.clone();
                        *message = "Réglages de la voix gardés : ils serviront aux prochains rendus.".into();
                    }
                    Err(e) => *message = format!("Réglages non écrits : {e}"),
                }
                self.ouverte = false;
            }
        });
        if !ouverte {
            self.reglages = self.gardes.clone();
            self.ouverte = false;
        }
        if change {
            self.a_refaire = Some(Instant::now());
        }
        lance
    }
}
