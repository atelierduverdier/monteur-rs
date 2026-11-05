//! La liste des prises : une image et la durée de chacune, calculées en fond (ffmpeg et ffprobe,
//! une prise après l'autre) pour que la fenêtre s'ouvre tout de suite.

use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

use dear_app::{ExternalTextureHandle, GpuApi};

pub const LARGEUR: u32 = 128;
pub const HAUTEUR: u32 = 80;

struct Recu {
    indice: usize,
    duree: Option<f64>,
    rgba: Option<Vec<u8>>,
}

struct Texture {
    _texture: wgpu::Texture,
    _vue: wgpu::TextureView,
    poignee: ExternalTextureHandle,
}

#[derive(Default)]
struct Detail {
    duree: Option<f64>,
    texture: Option<Texture>,
}

pub struct Miniatures {
    recus: Receiver<Recu>,
    details: Vec<Detail>,
}

fn sonder(chemin: &PathBuf) -> Recu {
    let duree = Command::new("ffprobe")
        .args(noyau::videos::commande_sonder(chemin).into_iter().skip(1))
        .output()
        .ok()
        .and_then(|s| noyau::videos::lire_infos(&String::from_utf8_lossy(&s.stdout)).ok())
        .map(|i| i.duree);
    // Trois secondes après le début : la première image est souvent celle d'avant le geste.
    let rgba = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", "3", "-i"])
        .arg(chemin)
        .args(["-frames:v", "1", "-vf", &format!("scale={LARGEUR}:{HAUTEUR}"), "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .ok()
        .map(|s| s.stdout)
        .filter(|o| o.len() == (LARGEUR * HAUTEUR * 4) as usize);
    Recu { indice: 0, duree, rgba }
}

impl Miniatures {
    pub fn lancer(prises: Vec<PathBuf>) -> Self {
        let (envoi, recus) = mpsc::channel();
        let details = prises.iter().map(|_| Detail::default()).collect();
        std::thread::spawn(move || {
            for (indice, chemin) in prises.iter().enumerate() {
                if envoi.send(Recu { indice, ..sonder(chemin) }).is_err() {
                    return;
                }
            }
        });
        Self { recus, details }
    }

    /// Range ce qui est arrivé ; à appeler à chaque tour, avant le dessin.
    pub fn recevoir(&mut self, gpu: &mut GpuApi<'_>) {
        while let Ok(r) = self.recus.try_recv() {
            let Some(d) = self.details.get_mut(r.indice) else { continue };
            d.duree = r.duree;
            let Some(rgba) = r.rgba else { continue };
            let taille = wgpu::Extent3d { width: LARGEUR, height: HAUTEUR, depth_or_array_layers: 1 };
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("miniature"),
                size: taille,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm, // comme la visionneuse : ImGui convertit
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            gpu.queue().write_texture(
                wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &rgba,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(LARGEUR * 4), rows_per_image: Some(HAUTEUR) },
                taille,
            );
            let vue = texture.create_view(&wgpu::TextureViewDescriptor::default());
            if let Ok(poignee) = gpu.register_external_texture(&vue) {
                d.texture = Some(Texture { _texture: texture, _vue: vue, poignee });
            }
        }
    }

    pub fn duree(&self, indice: usize) -> Option<f64> {
        self.details.get(indice).and_then(|d| d.duree)
    }

    /// Les identifiants ImGui des miniatures reçues, pour ce tour.
    pub fn textures(&self, gpu: &mut GpuApi<'_>) -> Vec<Option<dear_imgui_rs::TextureId>> {
        self.details
            .iter()
            .map(|d| d.texture.as_ref().and_then(|t| gpu.resolve_external_texture(t.poignee).ok()))
            .collect()
    }

    /// La carte graphique a été recréée, ou le dossier relu : les textures sont à oublier.
    pub fn liberer(&mut self, gpu: &mut GpuApi<'_>) {
        for d in &mut self.details {
            if let Some(t) = d.texture.take() {
                let _ = gpu.unregister_external_texture(t.poignee);
            }
        }
    }
}

/// « 2026-11-04 15-50-57 » devient « 4 nov. 2026, 15:50 » ; tout autre nom reste tel quel.
pub fn nom_lisible(nom: &str) -> String {
    const MOIS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    let o = nom.as_bytes();
    let chiffres = |a: usize, b: usize| o.get(a..b).filter(|s| s.iter().all(u8::is_ascii_digit)).and_then(|s| std::str::from_utf8(s).ok());
    if nom.len() == 19
        && o[4] == b'-'
        && o[7] == b'-'
        && let (Some(an), Some(m), Some(j), Some(h), Some(mi)) = (chiffres(0, 4), chiffres(5, 7), chiffres(8, 10), chiffres(11, 13), chiffres(14, 16))
        && let Some(mois) = m.parse::<usize>().ok().and_then(|m| MOIS.get(m.wrapping_sub(1)))
    {
        return format!("{} {mois} {an}, {h}:{mi}", j.trim_start_matches('0'));
    }
    nom.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms() {
        assert_eq!(nom_lisible("2026-11-04 15-50-57"), "4 nov. 2026, 15:50");
        assert_eq!(nom_lisible("2026-01-31 07-05-00"), "31 janv. 2026, 07:05");
        assert_eq!(nom_lisible("bug ignoré"), "bug ignoré");
        assert_eq!(nom_lisible("2026-13-31 07-05-00"), "2026-13-31 07-05-00");
    }
}
