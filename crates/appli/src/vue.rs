//! La visionneuse : l'image montée de l'instant, peinte par le composeur (le même dessin que le
//! rendu : ce qu'on voit est ce qui sortira), à la taille du panneau, puis envoyée à la carte
//! graphique. Elle ne se repeint que si quelque chose a changé.

use composeur::dessin::{Compositeur, R};
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

pub struct Vue {
    texture: Option<Texture>,
    sortie: Option<Pixmap>,
    /// La dernière image de la vidéo reçue, et son instant.
    source: Option<Pixmap>,
    source_t: f64,
    /// Ce qui a servi à la dernière peinture : rien n'a bougé, on ne repeint pas.
    cle: Option<(u64, u64, u32, u32, bool, u64)>,
    /// La taille voulue (celle du panneau au tour d'avant).
    pub taille: [u32; 2],
    /// Le temps d'une peinture (pour la barre d'état).
    pub dernier_ms: f64,
}

impl Vue {
    pub fn new() -> Self {
        Self { texture: None, sortie: None, source: None, source_t: -1.0, cle: None, taille: [960, 600], dernier_ms: 0.0 }
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
        let source = self.source.as_ref()?;
        // La place du panneau, aux proportions de la vidéo.
        let (sl, sh) = (source.width() as f32, source.height() as f32);
        let k = (self.taille[0] as f32 / sl).min(self.taille[1] as f32 / sh);
        let (l, h) = (((sl * k) as u32).max(16), ((sh * k) as u32).max(9));
        let cle = (self.source_t.to_bits(), prise.version, l, h, sans_zoom, t.to_bits());
        if self.cle != Some(cle) || self.texture.as_ref().is_none_or(|x| x.l != l || x.h != h) {
            let debut = std::time::Instant::now();
            let sortie = match &mut self.sortie {
                Some(p) if p.width() == l && p.height() == h => p,
                _ => self.sortie.insert(Pixmap::new(l, h)?),
            };
            sortie.fill(tiny_skia::Color::BLACK);
            Compositeur::new(&prise.montage, polices).peindre(
                &mut sortie.as_mut(),
                Some(source.as_ref()),
                t,
                R::new(0.0, 0.0, l as f64, h as f64),
                sans_zoom,
            );
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
