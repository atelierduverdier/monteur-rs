//! Essai 1 de l'étape 0 : une fenêtre Dear ImGui à panneaux ancrables sous Wayland, à la charte,
//! avec une image 1920 × 1080 renvoyée à la carte graphique à chaque tour, comme le fera la vidéo.
//!
//! `ESSAI_DUREE=10 cargo run --release -p essai-fenetre` : se ferme seul après 10 s et écrit ses
//! mesures (images par seconde, temps d'envoi de l'image) sur la sortie standard.

use std::time::{Duration, Instant};

use dear_app::{
    AppConfig, Application, ApplicationStage, DockingConfig, ExternalTextureHandle, FrameContext,
    GpuApi, GpuContext, InitContext, InitializedContext, RedrawMode, RunError, WgpuConfig,
    WgpuPreset, run,
};
use dear_imgui_rs::*;

const LARGEUR: u32 = 1920;
const HAUTEUR: u32 = 1080;

const INTER: &[u8] = include_bytes!("../../../ressources/polices/Inter-Regular.ttf");
const INTER_GRAS: &[u8] = include_bytes!("../../../ressources/polices/Inter-SemiBold.ttf");
const MONO: &[u8] = include_bytes!("../../../ressources/polices/JetBrainsMono-Medium.ttf");

struct Image {
    texture: wgpu::Texture,
    _vue: wgpu::TextureView,
    poignee: ExternalTextureHandle,
}

struct Panneaux {
    prises: WindowKey,
    visionneuse: WindowKey,
    inspecteur: WindowKey,
    ligne: WindowKey,
}

impl Panneaux {
    fn new() -> Self {
        let cle = |id, titre| WindowKey::new(id, titre).expect("clé de panneau valide");
        Self {
            prises: cle("prises", "Prises"),
            visionneuse: cle("visionneuse", "Visionneuse"),
            inspecteur: cle("inspecteur", "Inspecteur"),
            ligne: cle("ligne", "Ligne de temps"),
        }
    }

    /// La disposition de la maquette : prises à gauche, inspecteur à droite, ligne de temps en bas.
    fn disposition(&self) -> DockLayout {
        DockLayout::split(
            DockSplit::Down,
            0.32,
            DockLayout::tabs([&self.ligne]),
            DockLayout::split(
                DockSplit::Left,
                0.17,
                DockLayout::tabs([&self.prises]),
                DockLayout::split(
                    DockSplit::Right,
                    0.24,
                    DockLayout::tabs([&self.inspecteur]),
                    DockLayout::tabs([&self.visionneuse]),
                ),
            ),
        )
    }
}

#[derive(Default)]
struct Mesures {
    debut: Option<Instant>,
    images: u64,
    pire_image: Duration,
    envoi_total: Duration,
    dernier: Option<Instant>,
}

struct Essai {
    panneaux: Panneaux,
    image: Option<Image>,
    pixels: Vec<u8>,
    tete: f32, // position de la tête de lecture, 0..1
    lecture: bool,
    gras: Option<FontId>,
    mono: Option<FontId>,
    mesures: Mesures,
    duree: Option<Duration>,
}

impl Essai {
    fn new() -> Self {
        let duree = std::env::var("ESSAI_DUREE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .map(Duration::from_secs_f64);
        Self {
            panneaux: Panneaux::new(),
            image: None,
            pixels: mire(),
            tete: 0.204,
            lecture: true,
            gras: None,
            mono: None,
            mesures: Mesures::default(),
            duree,
        }
    }

    fn creer_image(&mut self, gpu: &mut GpuApi<'_>, etape: ApplicationStage) -> Result<(), RunError> {
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("image de la vidéo"),
            size: wgpu::Extent3d { width: LARGEUR, height: HAUTEUR, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let vue = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let poignee = gpu
            .register_external_texture(&vue)
            .map_err(|e| RunError::application(etape, e))?;
        self.image = Some(Image { texture, _vue: vue, poignee });
        Ok(())
    }

    /// Renvoie toute l'image à la carte, une bande orange glissant dessus : c'est le coût d'une
    /// image décodée par le processeur.
    fn envoyer_image(&mut self, gpu: &GpuApi<'_>) -> Duration {
        let Some(image) = &self.image else { return Duration::ZERO };
        let bande = (self.mesures.images as u32 * 8) % LARGEUR;
        for y in (0..HAUTEUR).step_by(2) {
            let ligne = (y * LARGEUR * 4) as usize;
            for x in [bande, (bande + LARGEUR - 8) % LARGEUR] {
                let i = ligne + (x * 4) as usize;
                let couleur: [u8; 4] = if x == bande { [255, 109, 0, 255] } else { [36, 36, 36, 255] };
                for k in 0..8 {
                    self.pixels[i + k * 4..i + k * 4 + 4].copy_from_slice(&couleur);
                }
            }
        }
        let debut = Instant::now();
        gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(LARGEUR * 4),
                rows_per_image: Some(HAUTEUR),
            },
            wgpu::Extent3d { width: LARGEUR, height: HAUTEUR, depth_or_array_layers: 1 },
        );
        debut.elapsed()
    }

    fn compter(&mut self, envoi: Duration) -> bool {
        let maintenant = Instant::now();
        let m = &mut self.mesures;
        let debut = *m.debut.get_or_insert(maintenant);
        if let Some(dernier) = m.dernier {
            // Les 60 premières images chauffent les tampons : on ne compte pas leur pire temps.
            if m.images > 60 {
                m.pire_image = m.pire_image.max(maintenant - dernier);
            }
        }
        m.dernier = Some(maintenant);
        m.images += 1;
        m.envoi_total += envoi;
        if let Some(duree) = self.duree
            && maintenant - debut >= duree
        {
            let s = (maintenant - debut).as_secs_f64();
            println!("images : {} en {:.2} s → {:.1} images/s", m.images, s, m.images as f64 / s);
            println!("pire écart entre deux images : {:.1} ms", m.pire_image.as_secs_f64() * 1e3);
            println!(
                "envoi d'une image 1920 × 1080 (write_texture, moyenne) : {:.2} ms",
                m.envoi_total.as_secs_f64() * 1e3 / m.images as f64
            );
            return true;
        }
        false
    }
}

/// Une fausse page de VerdierCAM : barre de titre, panneau gauche, dessin, carte à droite.
fn mire() -> Vec<u8> {
    let mut p = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        for x in 0..LARGEUR {
            let rgb = if y < 76 {
                charte::BG_TITLE
            } else if x < 345 {
                charte::BG_CARD
            } else if x > 1460 && x < 1880 && y > 130 && y < 780 {
                charte::BG_INPUT
            } else if (x / 40 + y / 40) % 2 == 0 {
                0x1E1E1E
            } else {
                0x202022
            };
            let i = ((y * LARGEUR + x) * 4) as usize;
            p[i] = (rgb >> 16) as u8;
            p[i + 1] = (rgb >> 8) as u8;
            p[i + 2] = rgb as u8;
            p[i + 3] = 255;
        }
    }
    p
}

fn temps(t: f32) -> String {
    let s = t * 408.0; // une prise de 6 min 48
    format!("{:02}:{:02}:{:05.2}", (s / 3600.0) as u32, (s / 60.0) as u32 % 60, s % 60.0)
}

impl Application for Essai {
    fn configure_imgui(&mut self, context: &mut InitContext<'_>) -> Result<(), RunError> {
        let etape = ApplicationStage::ConfigureImgui;
        let police = |octets| {
            StbTrueTypeFontData::from_slice(octets).map_err(|e| RunError::application(etape, e))
        };
        let (inter, gras, mono) = (police(INTER)?, police(INTER_GRAS)?, police(MONO)?);
        let imgui = context.imgui();
        let atlas = imgui.font_atlas();
        // La première police posée est celle de toute l'interface.
        atlas.add_font(&[FontSource::stb_truetype_with_size(inter, 15.0)]);
        self.gras = Some(atlas.add_font(&[FontSource::stb_truetype_with_size(gras, 15.0)]));
        self.mono = Some(atlas.add_font(&[FontSource::stb_truetype_with_size(mono, 15.0)]));
        charte::appliquer(imgui.style_mut());
        Ok(())
    }

    fn initialized(
        &mut self,
        _context: &mut InitializedContext<'_>,
        gpu: &mut GpuContext<'_>,
    ) -> Result<(), RunError> {
        self.creer_image(gpu.gpu(), ApplicationStage::Initialized)
    }

    fn gpu_lost(&mut self, gpu: &mut GpuContext<'_>) -> Result<(), RunError> {
        if let Some(image) = self.image.take() {
            gpu.gpu()
                .unregister_external_texture(image.poignee)
                .map_err(|e| RunError::application(ApplicationStage::GpuLost, e))?;
        }
        Ok(())
    }

    fn gpu_recreated(&mut self, gpu: &mut GpuContext<'_>) -> Result<(), RunError> {
        self.creer_image(gpu.gpu(), ApplicationStage::GpuRecreated)
    }

    fn frame(&mut self, context: &mut FrameContext<'_>) -> Result<(), RunError> {
        let envoi = self.envoyer_image(context.gpu());
        if self.compter(envoi) {
            context.request_exit();
        }
        let id_image = self
            .image
            .as_ref()
            .map(|i| context.gpu().resolve_external_texture(i.poignee))
            .transpose()
            .map_err(|e| RunError::application(ApplicationStage::Frame, e))?;
        let ui = context.ui();
        let (gras, mono) = (self.gras.unwrap(), self.mono.unwrap());
        if self.lecture {
            self.tete = (self.tete + ui.io().delta_time() / 408.0) % 1.0;
        }

        if let Some(barre) = ui.begin_main_menu_bar() {
            for menu in ["Fichier", "Édition", "Montage", "Voix", "Affichage", "Aide"] {
                if let Some(m) = ui.begin_menu(menu) {
                    ui.menu_item("(essai)");
                    m.end();
                }
            }
            barre.end();
        }

        // La fenêtre hôte des panneaux, sous la barre de menus.
        let vp = ui.main_viewport();
        ui.set_next_window_viewport(vp.id());
        let (pos, taille) = (vp.work_pos(), vp.work_size());
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
                resultat = ui
                    .dockspace()
                    .root_id(id)
                    .current_window(ui.content_region_avail())
                    .layout(&disposition, DockLayoutApply::IfMissing)
                    .build();
            });
        marge.pop();
        bord.pop();
        arrondi.pop();
        resultat.map_err(|e| RunError::application(ApplicationStage::Frame, e))?;

        ui.window(&self.panneaux.prises).build(|| {
            let prises = [
                ("2026-11-04 08-12-40", "brute", "6:48"),
                ("2026-11-03 07-55-02", "montée", "5:12"),
                ("2026-11-02 19-04-24", "vignette", "8:31"),
                ("2026-11-01 07-30-28", "envoyée", "4:05"),
            ];
            for (i, (nom, etat, duree)) in prises.iter().enumerate() {
                ui.selectable_config(nom).selected(i == 0).build();
                ui.text_colored(charte::c(charte::TEXT_2), format!("  {etat} · {duree}"));
            }
        });

        ui.window(&self.panneaux.visionneuse).build(|| {
            let dispo = ui.content_region_avail();
            let haut = dispo[1] - 40.0;
            let (mut l, mut h) = (dispo[0], dispo[0] * 9.0 / 16.0);
            if h > haut {
                h = haut;
                l = h * 16.0 / 9.0;
            }
            let x0 = ui.cursor_pos()[0] + (dispo[0] - l) * 0.5;
            ui.set_cursor_pos([x0, ui.cursor_pos()[1]]);
            if let Some(id) = id_image {
                ui.image(id, [l.max(16.0), h.max(9.0)]);
            }
            let _f = ui.push_font(mono);
            ui.text_colored(charte::c(charte::CHIFFRE_SAISIE), temps(self.tete));
            ui.same_line();
            ui.text_colored(charte::c(charte::UNIT), "/ 00:06:48.00");
            drop(_f);
            ui.same_line();
            if ui.button(if self.lecture { "Arrêter" } else { "Lire" }) {
                self.lecture = !self.lecture;
            }
            ui.same_line();
            ui.text_colored(
                charte::c(charte::TEXT_2),
                format!("{:.0} images/s", ui.io().framerate()),
            );
        });

        ui.window(&self.panneaux.inspecteur).build(|| {
            {
                let _f = ui.push_font(gras);
                ui.text("Zoom n° 4");
            }
            ui.separator();
            let mut grossissement = 1.8f32;
            let mut arrivee = 0.45f32;
            ui.slider("Grossissement", 1.0, 4.0, &mut grossissement);
            ui.slider("Temps d'arrivée", 0.1, 2.0, &mut arrivee);
            let mut suivre = true;
            ui.checkbox("Suivre la souris", &mut suivre);
        });

        ui.window(&self.panneaux.ligne).build(|| {
            ligne_de_temps(ui, &mut self.tete);
        });
        Ok(())
    }
}

/// La ligne de temps, dessinée à la main : la règle, huit pistes et la tête de lecture, qu'on
/// glisse à la souris.
fn ligne_de_temps(ui: &Ui, tete: &mut f32) {
    let pistes: [(&str, u32, &[(f32, f32, &str)]); 8] = [
        ("Image", charte::TEXT_2, &[(0.0, 0.07, "vignette"), (0.07, 1.0, "2026-11-04 08-12-40")]),
        ("Zooms", charte::PISTE_ZOOM, &[(0.09, 0.15, "×1,6"), (0.18, 0.25, "×1,8"), (0.52, 0.61, "boîte")]),
        ("Étiquettes", charte::PISTE_ETIQUETTE, &[(0.10, 0.14, "Usinage"), (0.19, 0.24, "Profondeur…")]),
        ("Titres", charte::TEXT, &[(0.005, 0.065, "Graver une image"), (0.92, 0.995, "Générique")]),
        ("Sous-titres", 0xBDBDBD, &[(0.08, 0.13, "Bon, on passe…"), (0.21, 0.27, "on règle…")]),
        ("Voix", charte::PISTE_VOIX, &[]),
        ("Musique", charte::PISTE_MUSIQUE, &[(0.0, 1.0, "Séquence Oxygène")]),
        ("Coupes", charte::PISTE_COUPE, &[(0.0, 0.03, "début"), (0.26, 0.28, "redite"), (0.96, 1.0, "fin")]),
    ];
    let dl = ui.get_window_draw_list();
    let origine = ui.cursor_screen_pos();
    let dispo = ui.content_region_avail();
    let entete = 130.0;
    let (x0, x1) = (origine[0] + entete, origine[0] + dispo[0]);
    let regle = 22.0;
    let hauteur_piste = ((dispo[1] - regle) / pistes.len() as f32).clamp(18.0, 40.0);

    dl.add_rect(origine, [x1, origine[1] + regle], charte::c(charte::BG_TITLE)).filled(true).build();
    for m in 0..=7 {
        let x = x0 + (x1 - x0) * m as f32 / 6.8;
        if x > x1 {
            break;
        }
        dl.add_line([x, origine[1]], [x, origine[1] + regle], charte::c(charte::BORDER)).build();
        dl.add_text([x + 4.0, origine[1] + 3.0], charte::c(charte::UNIT), format!("0{m}:00"));
    }
    for (n, (nom, couleur, blocs)) in pistes.iter().enumerate() {
        let y = origine[1] + regle + n as f32 * hauteur_piste;
        dl.add_rect([x0, y], [x1, y + hauteur_piste - 1.0], [0.102, 0.102, 0.102, 1.0])
            .filled(true)
            .build();
        dl.add_rect([origine[0] + 8.0, y + hauteur_piste * 0.5 - 4.0], [origine[0] + 16.0, y + hauteur_piste * 0.5 + 4.0], charte::c(*couleur))
            .filled(true)
            .rounding(2.0)
            .build();
        dl.add_text([origine[0] + 22.0, y + hauteur_piste * 0.5 - 8.0], charte::c(charte::TEXT), nom);
        if *nom == "Voix" {
            let mut x = x0;
            let mut i = 0;
            while x < x1 {
                let a = ((i as f32 * 0.37).sin() * (i as f32 * 0.11).cos()).abs() * 0.85 + 0.12;
                let demi = a * (hauteur_piste - 6.0) * 0.5;
                let milieu = y + hauteur_piste * 0.5;
                dl.add_line([x, milieu - demi], [x, milieu + demi], charte::vec(charte::PISTE_VOIX, 0.85)).build();
                x += 3.0;
                i += 1;
            }
        }
        for (a, b, texte) in blocs.iter() {
            let (bx0, bx1) = (x0 + (x1 - x0) * a, x0 + (x1 - x0) * b);
            dl.add_rect([bx0, y + 3.0], [bx1, y + hauteur_piste - 4.0], charte::vec(*couleur, 0.2))
                .filled(true)
                .rounding(3.0)
                .build();
            dl.add_rect([bx0, y + 3.0], [bx1, y + hauteur_piste - 4.0], charte::c(*couleur))
                .rounding(3.0)
                .build();
            dl.add_text([bx0 + 5.0, y + hauteur_piste * 0.5 - 8.0], charte::c(charte::TEXT), texte);
        }
    }

    // La tête de lecture se glisse sur toute la hauteur.
    ui.set_cursor_screen_pos([x0, origine[1]]);
    ui.invisible_button("tete", [x1 - x0, dispo[1].max(1.0)]);
    if ui.is_item_active() {
        *tete = ((ui.io().mouse_pos()[0] - x0) / (x1 - x0)).clamp(0.0, 1.0);
    }
    let xt = x0 + (x1 - x0) * *tete;
    dl.add_line([xt, origine[1]], [xt, origine[1] + dispo[1]], charte::c(charte::ACCENT))
        .thickness(2.0)
        .build();
    dl.add_triangle([xt - 6.0, origine[1]], [xt + 6.0, origine[1]], [xt, origine[1] + 10.0], charte::c(charte::ACCENT))
        .filled(true)
        .build();
}

fn main() {
    let config = AppConfig {
        window_title: "Monteur — essai 1 : fenêtre".to_string(),
        window_size: (1600.0, 1000.0),
        present_mode: wgpu::PresentMode::Fifo,
        clear_color: charte::c(charte::BG_CANVAS),
        wgpu: WgpuConfig::from_preset(WgpuPreset::HighPerformance),
        docking: DockingConfig::application_managed(),
        ini_filename: None,
        redraw: RedrawMode::Poll,
        ..Default::default()
    };
    if let Err(e) = run(config, Essai::new()) {
        eprintln!("l'essai a échoué : {e}");
        std::process::exit(1);
    }
}
