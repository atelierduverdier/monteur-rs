//! La visionneuse : l'image montée de l'instant, peinte par le composeur (le même dessin que le
//! rendu : ce qu'on voit est ce qui sortira), à la taille du panneau, puis envoyée à la carte
//! graphique. Elle ne se repeint que si quelque chose a changé.

use composeur::dessin::{Compositeur, R, peindre_image};
use composeur::police::Polices;
use dear_app::{ExternalTextureHandle, GpuApi};
use tiny_skia::Pixmap;

use crate::etat::Prise;

struct Texture {
    texture: wgpu::Texture,
    _vue: wgpu::TextureView,
    poignee: ExternalTextureHandle,
    l: u32,
    h: u32,
}

/// Ce qui a servi à une peinture : l'image de la prise, la version du montage, la taille, sans zoom ?, l'instant,
/// l'image insérée et celle de la webcam.
type Cle = (u64, u64, u32, u32, bool, u64, u64, u64);

pub struct Vue {
    texture: Option<Texture>,
    sortie: Option<Pixmap>,
    /// La dernière image de la vidéo reçue, et son instant.
    source: Option<Pixmap>,
    source_t: f64,
    /// Ce qui a servi à la dernière peinture : rien n'a bougé, on ne repeint pas.
    cle: Option<Cle>,
    /// La dernière image d'une vidéo insérée, et son instant.
    insere: Option<Pixmap>,
    insere_t: f64,
    /// La dernière image de la webcam, et son instant.
    camera: Option<Pixmap>,
    camera_t: f64,
    /// La taille voulue (celle du panneau au tour d'avant).
    pub taille: [u32; 2],
    /// Le temps d'une peinture (pour la barre d'état).
    pub dernier_ms: f64,
}

impl Vue {
    pub fn new() -> Self {
        Self { texture: None, sortie: None, source: None, source_t: -1.0, cle: None, insere: None, insere_t: -1.0, camera: None, camera_t: -1.0, taille: [960, 600], dernier_ms: 0.0 }
    }

    pub fn oublier_source(&mut self) {
        self.source = None;
        self.source_t = -1.0;
        self.cle = None;
    }

    /// Peint l'image de l'instant si besoin et la donne à ImGui. `sans_zoom` : l'image entière
    /// (pour régler un zoom à l'arrêt).
    pub fn preparer(&mut self, gpu: &mut GpuApi<'_>, prise: &mut Prise, polices: &Polices, sans_zoom: bool) -> Option<dear_imgui_rs::TextureId> {
        let t = prise.lecteur.position();
        if let Some(image) = prise.lecteur.image()
            && (image.t != self.source_t || self.source.is_none())
        {
            let taille = tiny_skia::IntSize::from_wh(image.largeur, image.hauteur)?;
            // Opaque : RGBA droit et prémultiplié sont les mêmes octets.
            self.source = Pixmap::from_vec(image.rgba.clone(), taille);
            self.source_t = image.t;
        }
        // Une vidéo insérée remplace l'image de la prise.
        let dessus = prise.intercalee.is_none() && prise.montage.dessus_a(t).is_some();
        let intercalee = prise.intercalee.is_some();
        let mut insere_la = false;
        if let Some(image) = prise.image_inseree(t) {
            insere_la = true;
            if image.t != self.insere_t || self.insere.is_none() {
                let taille = tiny_skia::IntSize::from_wh(image.largeur, image.hauteur)?;
                self.insere = Pixmap::from_vec(image.rgba.clone(), taille);
                self.insere_t = image.t;
            }
        }
        let mut camera_la = false;
        if !insere_la && let Some(image) = prise.image_webcam(t) {
            camera_la = true;
            if image.t != self.camera_t || self.camera.is_none() {
                let taille = tiny_skia::IntSize::from_wh(image.largeur, image.hauteur)?;
                self.camera = Pixmap::from_vec(image.rgba.clone(), taille);
                self.camera_t = image.t;
            }
        }
        let source = self.source.as_ref()?;
        // La place du panneau, aux proportions de la vidéo.
        let (sl, sh) = (source.width() as f32, source.height() as f32);
        let k = (self.taille[0] as f32 / sl).min(self.taille[1] as f32 / sh);
        let (l, h) = (((sl * k) as u32).max(16), ((sh * k) as u32).max(9));
        let insere_cle = if insere_la { self.insere_t.to_bits() } else { u64::MAX };
        let camera_cle = if camera_la { self.camera_t.to_bits() } else { u64::MAX };
        let cle = (self.source_t.to_bits(), prise.version, l, h, sans_zoom, t.to_bits(), insere_cle, camera_cle);
        if self.cle != Some(cle) || self.texture.as_ref().is_none_or(|x| x.l != l || x.h != h) {
            let debut = std::time::Instant::now();
            let sortie = match &mut self.sortie {
                Some(p) if p.width() == l && p.height() == h => p,
                _ => self.sortie.insert(Pixmap::new(l, h)?),
            };
            sortie.fill(tiny_skia::Color::BLACK);
            let tout = R::new(0.0, 0.0, l as f64, h as f64);
            match self.insere.as_ref().filter(|_| insere_la) {
                Some(v) => {
                    // Une autre vidéo, sans la déformer. Zooms, clics et étiquettes parlent de
                    // l'écran filmé : ils se taisent ; la voix continue, ses sous-titres aussi.
                    let (vl, vh) = (v.width() as f64, v.height() as f64);
                    let k = (l as f64 / vl).min(h as f64 / vh);
                    let cible = R::new((l as f64 - vl * k) / 2.0, (h as f64 - vh * k) / 2.0, vl * k, vh * k);
                    peindre_image(&mut sortie.as_mut(), v.as_ref(), cible, (0.0, 0.0, vl, vh));
                    if dessus && !intercalee {
                        let voix = noyau::montage::Montage {
                            sous_titres: prise.montage.sous_titres.clone(),
                            reglages: prise.montage.reglages.clone(),
                            ..Default::default()
                        };
                        Compositeur::new(&voix, polices).peindre_calque(&mut sortie.as_mut(), t, tout, (0.0, 0.0, l as f64, h as f64), l as f64, h as f64);
                    }
                }
                None => {
                    let camera = self.camera.as_ref().filter(|_| camera_la).map(Pixmap::as_ref);
                    Compositeur::new(&prise.montage, polices).peindre_avec_webcam(&mut sortie.as_mut(), Some(source.as_ref()), camera, t, tout, sans_zoom)
                }
            }
            self.envoyer(gpu, l, h);
            self.cle = Some(cle);
            self.dernier_ms = debut.elapsed().as_secs_f64() * 1e3;
        }
        let x = self.texture.as_ref()?;
        gpu.resolve_external_texture(x.poignee).ok()
    }

    /// Écrit la dernière image peinte (pour les essais).
    pub fn enregistrer(&self, chemin: &str) {
        if let Some(s) = &self.sortie
            && let Err(e) = s.save_png(chemin)
        {
            eprintln!("{chemin} : {e}");
        }
    }

    /// La taille de l'image de la webcam montrée (pour la placer à la souris).
    pub fn taille_camera(&self) -> Option<(f64, f64)> {
        self.camera.as_ref().map(|c| (c.width() as f64, c.height() as f64))
    }

    pub fn taille_image(&self) -> Option<[f32; 2]> {
        self.texture.as_ref().map(|x| [x.l as f32, x.h as f32])
    }

    fn envoyer(&mut self, gpu: &mut GpuApi<'_>, l: u32, h: u32) {
        let Some(sortie) = &self.sortie else { return };
        if self.texture.as_ref().is_none_or(|x| x.l != l || x.h != h) {
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("visionneuse"),
                size: wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // En Unorm, pas en sRGB : le shader d'ImGui passe déjà toute couleur à la puissance
                // 2,2 quand l'écran est en sRGB (dear-imgui-wgpu, `gamma_for_format`). Une texture
                // sRGB était convertie deux fois : l'image sortait plus foncée (vu par Christophe le
                // 04/11/2026 ; moyenne 154 à l'écran pour 219 peinte).
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let vue = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let poignee = match self.texture.take() {
                Some(ancienne) => match gpu.update_external_texture(ancienne.poignee, &vue) {
                    Ok(()) => ancienne.poignee,
                    Err(_) => return,
                },
                None => match gpu.register_external_texture(&vue) {
                    Ok(p) => p,
                    Err(_) => return,
                },
            };
            self.texture = Some(Texture { texture, _vue: vue, poignee, l, h });
        }
        let Some(x) = &self.texture else { return };
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo { texture: &x.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            sortie.data(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: l, height: h, depth_or_array_layers: 1 },
        );
    }

    /// La carte graphique a été recréée : la texture est à refaire.
    pub fn perdre(&mut self, gpu: &mut GpuApi<'_>) {
        if let Some(x) = self.texture.take() {
            let _ = gpu.unregister_external_texture(x.poignee);
        }
        self.cle = None;
    }
}
