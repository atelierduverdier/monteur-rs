//! Le panneau Vignette : l'image de la vidéo, la part qui va à droite, les textes, comme le Monteur
//! 1 ; le dessin vient du composeur. Écrit `<vidéo> vignette YouTube.png` et `.jpg` à côté de la
//! vidéo. Une vignette faite ou retouchée ailleurs n'est pas remplacée sans confirmation.

use std::path::Path;
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
        let sortie = std::process::Command::new(&cmd[0]).args(&cmd[1..]).output().ok();
        let _ = envoi.send(sortie.and_then(|s| Pixmap::decode_png(&s.stdout).ok()));
    });
    recu
}

impl Vignette {
    pub fn new() -> Self {
        Self { texture: None, source: None, recu: None, sale: true, pixmap: None, confirmer: false, saisie: None, ecrite: false }
    }

    /// Une autre prise : on repart de l'image de sa fiche.
    pub fn charger(&mut self, video: &Path, fiche: &Fiche, duree: f64) {
        self.source = None;
        self.confirmer = false;
        self.recu = Some(extraire(video, fiche.temps, duree));
        self.sale = true;
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
            let image = vignette::vignette(fiche, source.map(|s| s.as_ref()), cadre_de(fiche), polices);
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
            if ui.button("Prendre l'image de la tête de lecture") {
                fiche.temps = noyau::arrondi::arrondi(t_tete, 3);
                self.recu = Some(extraire(video, fiche.temps, duree));
                change = true;
            }
            ui.separator();
            let l = ui.push_item_width(-1.0);
            let mut champ = |ui: &Ui, nom: &str, id: &str, valeur: &mut String| {
                ui.text(nom);
                if ui.input_text(id, valeur).build() {
                    change = true;
                    self.sale = true;
                }
            };
            champ(ui, "Titre", "##titre", &mut fiche.titre);
            champ(ui, "Sous-titre", "##sous_titre", &mut fiche.sous_titre);
            champ(ui, "Ligne 1", "##ligne1", &mut fiche.ligne1);
            champ(ui, "Ligne 2", "##ligne2", &mut fiche.ligne2);
            champ(ui, "Ligne 3", "##ligne3", &mut fiche.ligne3);
            drop(l);
            for (champ, trop) in vignette::debordements(fiche, polices) {
                ui.text_colored(charte::c(charte::ERREUR), format!("{champ} : {trop} pixels de trop, il mord sur l'image"));
            }
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
            ui.text_colored(charte::c(charte::UNIT), "Glisser l'image déplace le cadrage ; la molette resserre.");
        });
        change
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
        let qualite = |q: u32| {
            std::process::Command::new("magick")
                .arg(&png_p)
                .args(["-background", "white", "-flatten", "-quality", &q.to_string()])
                .arg(&jpg_p)
                .status()
                .is_ok_and(|s| s.success())
        };
        // YouTube refuse une vignette de plus de 2 Mio.
        let mut fait = qualite(92);
        if fait && std::fs::metadata(&jpg_p).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
            fait = qualite(80);
        }
        if !fait {
            let _ = std::fs::remove_file(&png_p);
            return "JPG non écrit (magick).".into();
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
