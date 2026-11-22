//! Le panneau Vignette : l'image de la vidéo, la part qui va à droite, les textes, comme le Monteur
//! 1 ; le dessin vient du composeur. Écrit `<vidéo> vignette YouTube.png` et `.jpg` à côté de la
//! vidéo. Une vignette faite ou retouchée ailleurs n'est pas remplacée sans confirmation.

use std::time::Instant;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use composeur::police::Polices;
use composeur::vignette::{self, BANDEAU};
use dear_app::{ExternalTextureHandle, GpuApi};
use dear_imgui_rs::{MouseButton, TextureId, Ui};
use noyau::fiches::Fiche;
use noyau::videos;
use noyau::{IMAGE_H, IMAGE_L, VIGNETTE_H, VIGNETTE_L};
use serde_json::Number;
use tiny_skia::Pixmap;

struct Texture {
    texture: wgpu::Texture,
    _vue: wgpu::TextureView,
    poignee: ExternalTextureHandle,
}

pub struct Vignette {
    /// La tête vue au dernier tour, depuis quand elle s'est posée, et le dernier tour dessiné.
    tete: Option<f64>,
    bouge: Option<Instant>,
    vu_a: Option<Instant>,
    texture: Option<Texture>,
    source: Option<Pixmap>,
    recu: Option<Receiver<Option<Pixmap>>>,
    /// Il faut repeindre l'aperçu.
    sale: bool,
    pixmap: Option<Pixmap>,
    /// Une vignette faite ailleurs est là : le prochain clic la remplace.
    confirmer: bool,
    /// Le cadre tenu à la souris : départ de la souris, cadre de départ.
    saisie: Option<([f32; 2], [i64; 4])>,
    /// Une vignette vient d'être écrite : la fiche et la liste sont à mettre à jour.
    pub ecrite: bool,
    /// Le nom et le logo du bandeau, gardés pour toutes les prises (`marque.json`, à côté des fiches).
    marque: vignette::Marque,
    nom_marque: String,
    logo_marque: Option<PathBuf>,
    logo_choisi: Option<Receiver<Option<PathBuf>>>,
}

fn chemin_marque() -> PathBuf {
    noyau::systeme::donnees().join("marque.json")
}

/// La marque gardée : nom et chemin du logo ; VerdierCAM sinon.
fn lire_marque() -> (vignette::Marque, Option<PathBuf>) {
    let lu = std::fs::read_to_string(chemin_marque()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let nom = lu.as_ref().and_then(|v| v.get("nom")?.as_str().map(str::to_string)).filter(|n| !n.trim().is_empty());
    let logo = lu.as_ref().and_then(|v| v.get("logo")?.as_str().map(PathBuf::from));
    match nom {
        None => (vignette::Marque::default(), None),
        Some(nom) => {
            let image = logo.as_ref().and_then(|l| Pixmap::load_png(l).ok());
            (vignette::Marque { nom, logo: image }, logo)
        }
    }
}

pub fn cadre_de(fiche: &Fiche) -> Option<[i64; 4]> {
    let c = fiche.cadre.as_ref()?;
    let v: Vec<i64> = c.iter().map(|n| n.as_f64().unwrap_or(0.0) as i64).collect();
    (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
}

fn poser_cadre(fiche: &mut Fiche, c: [i64; 4]) {
    // En entiers : le Monteur 1 relit ce champ tel quel.
    fiche.cadre = Some(c.iter().map(|&x| Number::from(x)).collect());
}

/// L'image de la vidéo à la seconde `t`, en PNG par ffmpeg, décodée dans un fil.
fn extraire(video: &Path, t: f64, duree: f64) -> Receiver<Option<Pixmap>> {
    let (envoi, recu) = mpsc::channel();
    let cmd = videos::commande_image(video, t, Some(duree));
    std::thread::spawn(move || {
        let sortie = noyau::systeme::commande(&cmd[0]).args(&cmd[1..]).output().ok();
        let _ = envoi.send(sortie.and_then(|s| Pixmap::decode_png(&s.stdout).ok()));
    });
    recu
}

/// L'image posée sur du blanc, en RGB : les pixels de tiny-skia sont prémultipliés, le blanc passe
/// donc là où l'image est transparente (ce que faisait `magick -background white -flatten`).
fn a_plat_sur_blanc(rgba_premultiplie: &[u8]) -> Vec<u8> {
    rgba_premultiplie
        .chunks_exact(4)
        .flat_map(|p| {
            let fond = 255 - p[3];
            [p[0].saturating_add(fond), p[1].saturating_add(fond), p[2].saturating_add(fond)]
        })
        .collect()
}

/// La vignette en JPEG. Comme ImageMagick : à partir de 90, les couleurs ne sont pas
/// sous-échantillonnées (le texte reste net) ; en dessous, en 4:2:0.
fn en_jpeg(image: &Pixmap, qualite: u8) -> Result<Vec<u8>, String> {
    let (l, h) = (u16::try_from(image.width()).map_err(|e| e.to_string())?, u16::try_from(image.height()).map_err(|e| e.to_string())?);
    let mut sortie = Vec::new();
    jpeg_encoder::Encoder::new(&mut sortie, qualite)
        .encode(&a_plat_sur_blanc(image.data()), l, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(sortie)
}

impl Vignette {
    pub fn new() -> Self {
        let (marque, logo_marque) = lire_marque();
        let nom_marque = marque.nom.clone();
        Self {
            tete: None,
            bouge: None,
            vu_a: None,
            texture: None,
            source: None,
            recu: None,
            sale: true,
            pixmap: None,
            confirmer: false,
            saisie: None,
            ecrite: false,
            marque,
            nom_marque,
            logo_marque,
            logo_choisi: None,
        }
    }

    /// Une autre prise : on repart de l'image de sa fiche.
    pub fn charger(&mut self, video: &Path, fiche: &Fiche, duree: f64) {
        self.source = None;
        self.confirmer = false;
        self.tete = None;
        self.bouge = None;
        self.recu = Some(extraire(video, fiche.temps, duree));
        self.sale = true;
    }

    /// Onglet ouvert : la tête posée ailleurs dans la ligne de temps choisit l'image. Pas pendant une
    /// lecture (la tête ne se pose jamais), ni en revenant sur l'onglet.
    fn suivre_la_tete(&mut self, t: f64) -> bool {
        let revenu = self.vu_a.is_none_or(|v| v.elapsed().as_secs_f64() > 0.5);
        self.vu_a = Some(Instant::now());
        if revenu || self.tete.is_none() {
            self.tete = Some(t);
            self.bouge = None;
            return false;
        }
        if self.tete.is_some_and(|v| (v - t).abs() > 1e-3) {
            self.tete = Some(t);
            self.bouge = Some(Instant::now());
            return false;
        }
        if self.bouge.is_some_and(|b| b.elapsed().as_secs_f64() > 0.2) {
            self.bouge = None;
            return true;
        }
        false
    }

    pub fn liberer(&mut self, gpu: &mut GpuApi<'_>) {
        if let Some(t) = self.texture.take() {
            let _ = gpu.unregister_external_texture(t.poignee);
        }
        self.sale = true;
    }

    /// Repeint l'aperçu si besoin, avant le dessin du tour.
    pub fn preparer(&mut self, gpu: &mut GpuApi<'_>, fiche: Option<&mut Fiche>, polices: &Polices) -> Option<TextureId> {
        if let Some(r) = &self.recu
            && let Ok(p) = r.try_recv()
        {
            self.source = p;
            self.recu = None;
            self.sale = true;
        }
        if let (Some(fiche), true) = (fiche, self.sale) {
            let source = self.source.as_ref();
            if let (Some(s), None) = (source, fiche.cadre.as_ref()) {
                poser_cadre(fiche, videos::cadre_defaut(s.width() as i64, s.height() as i64));
            }
            let image = vignette::vignette_a_la_marque(fiche, source.map(|s| s.as_ref()), cadre_de(fiche), polices, &self.marque);
            self.envoyer(gpu, &image);
            self.pixmap = Some(image);
            self.sale = false;
        }
        self.texture.as_ref().and_then(|t| gpu.resolve_external_texture(t.poignee).ok())
    }

    fn envoyer(&mut self, gpu: &mut GpuApi<'_>, image: &Pixmap) {
        let taille = wgpu::Extent3d { width: image.width(), height: image.height(), depth_or_array_layers: 1 };
        if self.texture.is_none() {
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("vignette"),
                size: taille,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let vue = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let Ok(poignee) = gpu.register_external_texture(&vue) else { return };
            self.texture = Some(Texture { texture, _vue: vue, poignee });
        }
        let Some(t) = &self.texture else { return };
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            image.data(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(image.width() * 4), rows_per_image: Some(image.height()) },
            taille,
        );
    }

    /// Le panneau. Rend vrai si la fiche a changé.
    pub fn dessiner(
        &mut self,
        ui: &Ui,
        id: Option<TextureId>,
        fiche: &mut Fiche,
        video: &Path,
        t_tete: f64,
        duree: f64,
        polices: &Polices,
        message: &mut String,
    ) -> bool {
        let mut change = false;
        let dispo = ui.content_region_avail();
        // L'aperçu à gauche, les textes à droite.
        let large = (dispo[0] - 340.0).max(320.0);
        let haut = (large * VIGNETTE_H as f32 / VIGNETTE_L as f32).min(dispo[1] - 8.0).max(90.0);
        let large = haut * VIGNETTE_L as f32 / VIGNETTE_H as f32;
        let debut = ui.cursor_pos();
        if let Some(id) = id {
            ui.image(id, [large, haut]);
            let coin = ui.item_rect_min();
            // Glisser dans la partie droite déplace ce qu'on y voit ; la molette resserre.
            if let (Some(s), Some(c)) = (&self.source, cadre_de(fiche)) {
                let (sl, sh) = (s.width() as i64, s.height() as i64);
                let souris = ui.io().mouse_pos();
                let x_image = coin[0] + BANDEAU as f32 / VIGNETTE_L as f32 * large;
                if ui.is_item_hovered() && souris[0] >= x_image {
                    if ui.is_mouse_clicked(MouseButton::Left) {
                        self.saisie = Some((souris, c));
                    }
                    let m = ui.io().mouse_wheel();
                    if m != 0.0 {
                        let l = c[2] as f64 * if m > 0.0 { 0.92 } else { 1.0 / 0.92 };
                        let (cx, cy) = (c[0] as f64 + c[2] as f64 / 2.0, c[1] as f64 + c[3] as f64 / 2.0);
                        let h = l * IMAGE_H as f64 / IMAGE_L as f64;
                        poser_cadre(fiche, videos::borner_cadre([cx - l / 2.0, cy - h / 2.0, l, h], sl, sh));
                        (change, self.sale) = (true, true);
                    }
                }
                if !ui.is_mouse_down(MouseButton::Left) {
                    self.saisie = None;
                }
                if let Some((depart, c0)) = self.saisie {
                    // Un pixel de l'aperçu vaut tant de pixels de la vidéo ; l'image suit la main.
                    let k = c0[2] as f64 / (IMAGE_L as f64 / VIGNETTE_L as f64 * large as f64);
                    let x = c0[0] as f64 - (souris[0] - depart[0]) as f64 * k;
                    let y = c0[1] as f64 - (souris[1] - depart[1]) as f64 * k;
                    let n = videos::borner_cadre([x, y, c0[2] as f64, c0[3] as f64], sl, sh);
                    if n != c {
                        poser_cadre(fiche, n);
                        (change, self.sale) = (true, true);
                    }
                }
            }
        }
        ui.set_cursor_pos([debut[0] + large + 12.0, debut[1]]);
        ui.child_window("textes").size([0.0, 0.0]).build(ui, || {
            ui.text_colored(charte::c(charte::TEXT_2), format!("Image prise à {}", crate::etat::temps(fiche.temps)));
            if self.suivre_la_tete(t_tete) {
                fiche.temps = noyau::arrondi::arrondi(t_tete, 3);
                self.recu = Some(extraire(video, fiche.temps, duree));
                change = true;
            }
            ui.text_wrapped("Clique dans la ligne de temps : l'image de cet instant vient dans la vignette.");
            ui.separator();
            let l = ui.push_item_width(-1.0);
            let champ = |ui: &Ui, nom: &str, id: &str, valeur: &mut String| {
                ui.text(nom);
                ui.input_text(id, valeur).build()
            };
            let mut touche = champ(ui, "Titre", "##titre", &mut fiche.titre);
            let mut corps = vignette::corps_de(fiche, "titre", &vignette::STYLES[0].1, polices);
            ui.set_next_item_width(ui.content_region_avail()[0] - 60.0);
            let taille = dear_imgui_rs::NumericFormat::<f32>::new("Taille %.0f".to_string());
            let bouge = match taille {
                Ok(f) => ui.slider_config("##corps", 40.0, 140.0).display_format(f).build(&mut corps),
                Err(_) => ui.slider_f32("##corps", &mut corps, 40.0, 140.0),
            };
            if bouge {
                fiche.corps_titre = corps.round() as f64;
                change = true;
                self.sale = true;
            }
            ui.same_line();
            if ui.button("Auto") && fiche.corps_titre != 0.0 {
                fiche.corps_titre = 0.0;
                change = true;
                self.sale = true;
            }
            if ui.is_item_hovered() {
                ui.tooltip_text("La plus grande taille qui tient dans le bandeau.");
            }
            touche |= champ(ui, "Sous-titre", "##sous_titre", &mut fiche.sous_titre);
            touche |= champ(ui, "Ligne 1", "##ligne1", &mut fiche.ligne1);
            touche |= champ(ui, "Ligne 2", "##ligne2", &mut fiche.ligne2);
            touche |= champ(ui, "Ligne 3", "##ligne3", &mut fiche.ligne3);
            if touche {
                change = true;
                self.sale = true;
            }
            drop(l);
            for (champ, trop) in vignette::debordements(fiche, polices) {
                ui.text_colored(charte::c(charte::ERREUR), format!("{champ} : {trop} pixels de trop, il mord sur l'image"));
            }
            self.panneau_marque(ui, message);
            ui.separator();
            let png = noyau::chemin_derive(video, noyau::SUFFIXE_PNG);
            let texte = if self.confirmer { "Remplacer la vignette" } else { "Écrire la vignette" };
            if ui.button_with_size(texte, [-1.0, 32.0]) {
                *message = self.ecrire(fiche, video, &png);
                change = true;
            }
            if self.confirmer {
                ui.text_wrapped("Une vignette faite ou retouchée hors du Monteur est déjà là.");
            }
            ui.text_wrapped("Écrite à côté de la vidéo, la vignette passe 3 s en tête au prochain rendu, et YouTube la reçoit à l'envoi.");
            ui.text_colored(charte::c(charte::UNIT), "Glisser l'image déplace le cadrage ; la molette resserre.");
        });
        change
    }

    /// La chaîne : son nom et son logo en haut du bandeau, à la place de ceux de VerdierCAM.
    fn panneau_marque(&mut self, ui: &Ui, message: &mut String) {
        if let Some(r) = &self.logo_choisi
            && let Ok(choix) = r.try_recv()
        {
            self.logo_choisi = None;
            if let Some(chemin) = choix {
                match Pixmap::load_png(&chemin) {
                    Ok(image) => {
                        self.marque.logo = Some(image);
                        self.logo_marque = Some(chemin);
                        self.garder_marque(message);
                    }
                    Err(e) => *message = format!("Logo illisible (un PNG est attendu) : {e}"),
                }
            }
        }
        if !ui.collapsing_header("Chaîne", dear_imgui_rs::TreeNodeFlags::empty()) {
            return;
        }
        ui.text_colored(charte::c(charte::TEXT_2), "Le nom et le logo du haut du bandeau, pour toutes les prises.");
        ui.set_next_item_width(-1.0);
        if ui.input_text("##marque", &mut self.nom_marque).build() && !self.nom_marque.trim().is_empty() {
            self.marque.nom = self.nom_marque.trim().to_string();
            self.garder_marque(message);
        }
        if ui.button("Logo…") && self.logo_choisi.is_none() {
            let (envoi, recu) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = envoi.send(crate::choisir_un_fichier(&noyau::systeme::maison(), "Le logo de la chaîne (PNG)", ("Images PNG", &["png", "PNG"])));
            });
            self.logo_choisi = Some(recu);
        }
        ui.same_line();
        if ui.button("Revenir à VerdierCAM") {
            self.marque = vignette::Marque::default();
            self.nom_marque = self.marque.nom.clone();
            self.logo_marque = None;
            let _ = std::fs::remove_file(chemin_marque());
            self.sale = true;
        }
    }

    fn garder_marque(&mut self, message: &mut String) {
        self.sale = true;
        let valeur = serde_json::json!({ "nom": self.marque.nom, "logo": self.logo_marque.as_ref().map(|l| l.to_string_lossy().into_owned()) });
        let chemin = chemin_marque();
        let ecrit = chemin.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&chemin, valeur.to_string()));
        if let Err(e) = ecrit {
            *message = format!("Chaîne non gardée : {e}");
        }
    }

    fn ecrire(&mut self, fiche: &mut Fiche, video: &Path, png: &Path) -> String {
        if fiche.titre.trim().is_empty() {
            return "Il faut au moins un titre.".into();
        }
        let Some(image) = &self.pixmap else { return "L'aperçu n'est pas prêt.".into() };
        let date = |p: &Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i64)
        };
        let jpg = noyau::chemin_derive(video, noyau::SUFFIXE_JPG);
        let a_nous = date(png) == Some(fiche.vignette_ns);
        if (png.exists() || jpg.exists()) && !a_nous && !self.confirmer {
            self.confirmer = true;
            return "Une vignette faite ailleurs est déjà là : cliquer de nouveau pour la remplacer.".into();
        }
        self.confirmer = false;
        // Un temporaire puis un renommage : une coupure laisse l'ancienne entière.
        let (png_p, jpg_p) = (videos::chemin_partiel(png), videos::chemin_partiel(&jpg));
        if let Err(e) = image.save_png(&png_p) {
            return format!("Vignette non écrite : {e}");
        }
        // YouTube refuse une vignette de plus de 2 Mio.
        let jpeg = en_jpeg(image, 92).and_then(|j| if j.len() > 2 * 1024 * 1024 { en_jpeg(image, 80) } else { Ok(j) });
        if let Err(e) = jpeg.and_then(|j| std::fs::write(&jpg_p, j).map_err(|e| e.to_string())) {
            let _ = std::fs::remove_file(&png_p);
            let _ = std::fs::remove_file(&jpg_p);
            return format!("JPG non écrit : {e}");
        }
        if let Err(e) = std::fs::rename(&png_p, png).and_then(|_| std::fs::rename(&jpg_p, &jpg)) {
            return format!("Vignette non posée à côté de la vidéo : {e}");
        }
        fiche.vignette_ns = date(png).unwrap_or(0);
        self.ecrite = true;
        format!("Vignette écrite : {} et .jpg", png.file_name().unwrap_or_default().to_string_lossy())
    }

    /// Pour les essais : comme un clic sur « Écrire la vignette ».
    pub fn ecrire_pour_essai(&mut self, fiche: &mut Fiche, video: &Path) -> String {
        let png = noyau::chemin_derive(video, noyau::SUFFIXE_PNG);
        self.ecrire(fiche, video, &png)
    }

    /// À repeindre (une fiche changée d'ailleurs).
    pub fn salir(&mut self) {
        self.sale = true;
    }

    /// Pour les essais : l'aperçu tel qu'il s'écrira.
    pub fn enregistrer(&self, chemin: &str) {
        if let Some(p) = &self.pixmap {
            let _ = p.save_png(chemin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_transparent_devient_blanc() {
        // Opaque rouge, transparent, demi-transparent noir (prémultiplié : 0, alpha 128).
        let rgb = a_plat_sur_blanc(&[255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128]);
        assert_eq!(rgb, vec![255, 0, 0, 255, 255, 255, 127, 127, 127]);
    }

    #[test]
    fn un_jpeg_entier() {
        let mut p = Pixmap::new(1280, 720).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(0x1E, 0x1E, 0x1E, 255));
        let j = en_jpeg(&p, 92).unwrap();
        assert_eq!(&j[..2], &[0xFF, 0xD8], "début d'un JPEG");
        assert_eq!(&j[j.len() - 2..], &[0xFF, 0xD9], "fin d'un JPEG");
        assert!(en_jpeg(&p, 80).unwrap().len() <= j.len());
    }
}
