//! Le Monteur 2 : la fenêtre. Prises à gauche, visionneuse au milieu, inspecteur à droite, ligne de
//! temps en bas (la maquette Design du 04/11/2026). Le montage s'écrit seul à côté de la vidéo,
//! au même format que le Monteur 1 ; la vidéo n'est jamais touchée.
//!
//! `monteur2 [dossier]` : sans dossier, celui des fiches du Monteur 1, sinon « Copies d'écran ».

mod capture;
mod etat;
mod ligne;
mod miniatures;
mod outils;
mod raccourci;
mod scenario;
mod vignette;
mod voix;
mod vue;
mod youtube;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

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
    vignette: WindowKey,
    youtube: WindowKey,
    texte: WindowKey,
}

impl Panneaux {
    fn new() -> Self {
        let cle = |id, titre| WindowKey::new(id, titre).expect("clé de panneau");
        Self {
            prises: cle("prises", "Prises"),
            visionneuse: cle("visionneuse", "Visionneuse"),
            inspecteur: cle("inspecteur", "Inspecteur"),
            ligne: cle("ligne", "Ligne de temps"),
            vignette: cle("vignette", "Vignette"),
            youtube: cle("youtube", "YouTube"),
            texte: cle("texte", "Texte"),
        }
    }

    fn disposition(&self) -> DockLayout {
        DockLayout::split(
            DockSplit::Down,
            0.38,
            DockLayout::tabs([&self.ligne]),
            DockLayout::split(
                DockSplit::Left,
                0.17,
                DockLayout::tabs([&self.prises]),
                DockLayout::split(DockSplit::Right, 0.24, DockLayout::tabs([&self.texte, &self.inspecteur]), DockLayout::tabs([&self.visionneuse, &self.vignette, &self.youtube])),
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
const RUBAN: f32 = 64.0;

/// Un rendu en fond : on continue de monter pendant ce temps.
/// Un rendu de la file : montage et voix figés au moment où il y entre.
enum EtatTravail {
    Attente,
    EnCours,
    Fini { reussi: bool, secondes: f64 },
}

struct Travail {
    nom: String,
    video: PathBuf,
    montage: noyau::montage::Montage,
    voix: noyau::voix::Voix,
    pre: usize,
    etat: EtatTravail,
}

struct RenduEnCours {
    nom: String,
    suivi: Arc<rendu::Suivi>,
    fin: Receiver<Result<String, String>>,
    fil: std::thread::JoinHandle<()>,
    debut: std::time::Instant,
}

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
    fiches: Fiches,
    /// Une fiche changée, pas encore écrite.
    fiches_sales: bool,
    /// Un dossier à ouvrir au tour suivant.
    a_changer: Option<PathBuf>,
    /// Où sont les projets, et lesquels.
    racine: PathBuf,
    projets: Vec<PathBuf>,
    /// Le dossier de départ (les prises d'OBS).
    copies: PathBuf,
    /// Le dossier a changé : les miniatures sont à refaire.
    relire: bool,
    /// « Nouveau projet » : ouvrir la boîte au prochain tour, et le nom tapé.
    ouvrir_boite: bool,
    nom_projet: String,
    vignette: vignette::Vignette,
    bande: miniatures::Bande,
    bande_neuve: bool,
    rendu: Option<RenduEnCours>,
    file: Vec<Travail>,
    fenetre_rendre: bool,
    pre_choisi: usize,
    voix: voix::FenetreVoix,
    /// L'écran en train d'être filmé.
    capture: Option<capture::Capture>,
    /// Filmer VerdierCAM : l'enregistrement attend que sa fenêtre soit ouverte.
    capture_prevue: Option<std::time::Instant>,
    /// Le raccourci global, demandé au premier enregistrement.
    raccourci: Option<Receiver<raccourci::Evenement>>,
    /// La prise dont on demande la mise à la corbeille, en attente de confirmation.
    a_jeter: Option<PathBuf>,
    ouvrir_corbeille: bool,
    /// Un enregistrement simple : il finit dans Copies d'écran, sans toucher au travail en cours.
    capture_simple: bool,
    /// La prise tout juste filmée : Préparer se lance dès qu'elle est ouverte.
    a_preparer: Option<PathBuf>,
    /// Le panneau Texte : ce qu'on y cherche, et la phrase que la lecture y a amenée.
    cherche: String,
    phrase_suivie: Option<usize>,
    youtube: youtube::Youtube,
    /// Pour les essais : passer sur l'onglet YouTube.
    montrer_youtube: bool,
    montrer_texte: bool,
    montrer_vignette: bool,
    envoi: Option<youtube::Envoi>,
    /// Quand on a regardé le dossier pour la dernière fois.
    regarde: std::time::Instant,
    /// Une prise ou un fichier projet choisis dans le dialogue de KDE.
    prise_choisie: Option<Receiver<String>>,
    projet_choisi: Option<Receiver<String>>,
    /// Une vidéo qu'on importe, en fond : son chemin relatif, sa durée, par-dessus ?, l'instant.
    import: Option<Receiver<Result<Option<(String, f64, bool, f64)>, String>>>,
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

/// Le temps laissé à VerdierCAM pour ouvrir sa fenêtre : le dialogue ne liste que celles qui existent.
const DELAI_VERDIERCAM: std::time::Duration = std::time::Duration::from_secs(4);

fn chemin_voix() -> PathBuf {
    chemin_fiches().with_file_name("voix.json")
}

fn chemin_jeton_portail() -> PathBuf {
    chemin_fiches().with_file_name("portail-capture")
}

/// Le niveau du micro : vert, orange près de saturer, rouge saturé.
fn vu_metre(ui: &Ui, db: f32, muet: bool) {
    let (a, b) = (ui.cursor_screen_pos(), [ui.cursor_screen_pos()[0] + 110.0, ui.cursor_screen_pos()[1] + 30.0]);
    ui.dummy([110.0, 30.0]);
    let (haut, bas) = (a[1] + 10.0, b[1] - 10.0);
    let dl = ui.get_window_draw_list();
    dl.add_rect([a[0], haut], [b[0], bas], charte::c(charte::BG_INPUT)).filled(true).rounding(2.0).build();
    let part = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let couleur = if db > -1.0 { charte::ERREUR } else if db > -6.0 { charte::ALERTE } else { charte::SUCCES };
    dl.add_rect([a[0], haut], [a[0] + 110.0 * part, bas], charte::c(couleur)).filled(true).rounding(2.0).build();
    if muet {
        ui.same_line();
        ui.align_text_to_frame_padding();
        ui.text_colored(charte::c(charte::ALERTE), "Micro muet ?");
    }
}

fn chrono_court(s: f64) -> String {
    format!("{}:{:02}", s as u64 / 60, s as u64 % 60)
}

fn chemin_fiches() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".local/share"))
        .join("monteur/fiches.json")
}

impl Monteur {
    fn new() -> Self {
        // Un fichier de fiches illisible est mis de côté, jamais écrasé (comme le Monteur 1).
        let fiches = Fiches::lire(&chemin_fiches()).unwrap_or_else(|e| {
            let cote = chemin_fiches().with_extension(format!("json.abime-{}", std::process::id()));
            let _ = std::fs::rename(chemin_fiches(), &cote);
            eprintln!("Fiches illisibles ({e}) : mises de côté sous {}", cote.display());
            Fiches::default()
        });
        let copies = dossier_de_depart(&fiches);
        let dossier = Dossier::lire(&copies, &fiches);
        let racine = std::env::var_os("MONTEUR2_PROJETS").map(PathBuf::from).unwrap_or_else(noyau::projets::racine);
        let projets = noyau::projets::lister(&racine);
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
            fiches,
            fiches_sales: false,
            a_changer: None,
            racine,
            projets,
            copies,
            relire: false,
            ouvrir_boite: false,
            nom_projet: String::new(),
            vignette: vignette::Vignette::new(),
            bande: miniatures::Bande::vide(),
            bande_neuve: false,
            rendu: None,
            file: Vec::new(),
            fenetre_rendre: false,
            pre_choisi: 0,
            voix: voix::FenetreVoix::new(&chemin_voix()),
            capture: None,
            capture_prevue: None,
            raccourci: None,
            capture_simple: false,
            a_jeter: None,
            ouvrir_corbeille: false,
            a_preparer: None,
            cherche: String::new(),
            phrase_suivie: None,
            youtube: youtube::Youtube::new(),
            montrer_youtube: false,
            montrer_texte: false,
            montrer_vignette: false,
            envoi: None,
            regarde: std::time::Instant::now(),
            prise_choisie: None,
            projet_choisi: None,
            import: None,
            scenario: scenario::Scenario::depuis_l_environnement(),
        }
    }

    fn ecrire_fiches(&mut self) {
        self.fiches_sales = false;
        if let Err(e) = self.fiches.ecrire(&chemin_fiches(), &noyau::videos::dossier_defaut(), &noyau::videos::rangement_defaut()) {
            self.message = format!("Fiches non écrites : {e}");
        }
    }

    fn changer_de_dossier(&mut self, chemin: PathBuf) {
        self.dossier = Dossier::lire(&chemin, &self.fiches);
        self.projets = noyau::projets::lister(&self.racine);
        self.relire = true;
    }

    /// La prise ouverte part dans le projet `nom`, avec tout ce qui l'accompagne.
    /// Le dossier affiché, dans le gestionnaire de fichiers.
    fn ouvrir_le_dossier(&mut self) {
        let lance = std::process::Command::new("xdg-open")
            .arg(&self.dossier.chemin)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Err(e) = lance {
            self.message = format!("Dossier non ouvert : {e}");
        }
    }

    fn ranger_dans_un_projet(&mut self, nom: &str) {
        let Some(mut p) = self.prise.take() else { return };
        if let Some(e) = p.ecrire_si_besoin(true) {
            self.message = e;
        }
        let video = p.video.clone();
        drop(p);
        self.vue.oublier_source();
        self.ecrire_fiches();
        match noyau::projets::ranger_dans_un_projet(&mut self.fiches, &chemin_fiches(), &video, &self.racine, nom) {
            Ok(nouvelle) => {
                let dossier = nouvelle.parent().unwrap_or(&self.racine).to_path_buf();
                self.message = format!("Projet « {} » : la prise et ses fichiers sont maintenant dans {}.", nom.trim(), dossier.display());
                self.changer_de_dossier(dossier);
                self.a_ouvrir = Some(nouvelle);
            }
            Err(e) => {
                self.message = e.0;
                self.a_ouvrir = Some(video);
            }
        }
    }

    /// Écrit tout : le montage, les fiches, et le fichier projet `<vidéo>.monteur`.
    fn enregistrer(&mut self) {
        if self.fiches_sales {
            self.ecrire_fiches();
        }
        let Some(p) = &mut self.prise else { return };
        if let Some(e) = p.ecrire_si_besoin(true) {
            self.message = e;
            return;
        }
        let description = noyau::description::Description::lire(&noyau::chemin_derive(&p.video, noyau::SUFFIXE_DESCRIPTION)).unwrap_or_default();
        let projet = noyau::projet::Projet {
            video: p.video.clone(),
            fiche: self.fiches.fiches.get(&noyau::videos::cle(&p.video)).cloned().unwrap_or_default(),
            description,
            montage: Some(p.montage.en_valeur()),
            taille: std::fs::metadata(&p.video).map_or(0, |m| m.len()),
            ..Default::default()
        };
        let chemin = noyau::chemin_derive(&p.video, noyau::EXTENSION_PROJET);
        self.message = match projet.ecrire(&chemin, env!("CARGO_PKG_VERSION")) {
            Ok(()) => format!("Projet enregistré : {}", chemin.display()),
            Err(e) => format!("Projet non écrit : {e}"),
        };
    }

    /// Un projet tout neuf : la prise d'OBS ouverte y part ; sinon, un dossier vide où en ajouter.
    fn nouveau_projet(&mut self, nom: &str) {
        let hors_projet = self.prise.as_ref().is_some_and(|p| !p.dossier().starts_with(&self.racine));
        if hors_projet {
            self.ranger_dans_un_projet(nom);
            return;
        }
        let Some(nom) = noyau::projets::nom_de_projet(nom) else { return };
        let dossier = self.racine.join(&nom);
        if let Err(e) = std::fs::create_dir_all(&dossier) {
            self.message = format!("{} : {e}", dossier.display());
            return;
        }
        self.fermer_prise();
        self.changer_de_dossier(dossier);
        self.message = format!("Projet « {nom} » créé dans {} : Fichier › Ajouter une prise au projet.", self.dossier.chemin.display());
    }

    /// Retour à Copies d'écran : ce qu'on filme ensuite n'ira pas dans le projet.
    fn fermer_projet(&mut self) {
        self.fermer_prise();
        self.changer_de_dossier(self.copies.clone());
        self.message = "Projet fermé : les prochaines prises arrivent dans Copies d'écran.".into();
    }

    fn fermer_prise(&mut self) {
        if let Some(mut p) = self.prise.take()
            && let Some(e) = p.ecrire_si_besoin(true)
        {
            self.message = e;
        }
        if self.fiches_sales {
            self.ecrire_fiches();
        }
        self.vue.oublier_source();
    }

    /// Rend la prise ouverte en fond, telle qu'elle est maintenant.
    /// Le rendu de toujours, sans passer par la fenêtre.
    fn rendre(&mut self) {
        self.mettre_en_file(0, false);
    }

    /// `devant` : avant ceux qui attendent. Il part aussitôt si rien ne tourne.
    fn mettre_en_file(&mut self, pre: usize, devant: bool) {
        self.enregistrer();
        let Some(p) = &self.prise else { return };
        let travail = Travail {
            nom: miniatures::nom_lisible(&p.video.file_stem().unwrap_or_default().to_string_lossy()),
            video: p.video.clone(),
            montage: p.montage.clone(),
            voix: self.voix.reglages.clone(),
            pre,
            etat: EtatTravail::Attente,
        };
        let rang = if devant { self.file.iter().position(|t| matches!(t.etat, EtatTravail::Attente)).unwrap_or(self.file.len()) } else { self.file.len() };
        self.file.insert(rang, travail);
        self.lancer_suivant();
        self.message = "Rendu en file : tu peux continuer à monter pendant ce temps.".into();
    }

    fn lancer_suivant(&mut self) {
        if self.rendu.is_some() {
            return;
        }
        let Some(t) = self.file.iter_mut().find(|t| matches!(t.etat, EtatTravail::Attente)) else { return };
        t.etat = EtatTravail::EnCours;
        let (video, montage, voix, pre) = (t.video.clone(), t.montage.clone(), t.voix.clone(), noyau::prereglages::PREREGLAGES[t.pre]);
        let suivi = Arc::new(rendu::Suivi::default());
        let (envoi, fin) = mpsc::channel();
        let s = suivi.clone();
        let nom = t.nom.clone();
        let fil = std::thread::spawn(move || {
            let _ = envoi.send(rendu::monter(&video, &montage, &voix, &pre, &s).map_err(|e| e.0));
        });
        self.rendu = Some(RenduEnCours { nom, suivi, fin, fil, debut: std::time::Instant::now() });
    }

    /// Préréglages à gauche, ce qu'ils font à droite, la file en bas.
    fn panneau_rendre(&mut self, ui: &Ui) {
        use noyau::prereglages::PREREGLAGES;
        let bas = 190.0 + 6.0 * self.file.len().min(4) as f32;
        let haut = (ui.content_region_avail()[1] - bas).max(180.0);
        ui.child_window("presets").size([280.0, haut]).build(ui, || {
            ui.text_colored(charte::c(charte::TEXT_2), "Préréglages");
            for (i, p) in PREREGLAGES.iter().enumerate() {
                if ui.selectable_config(format!("##pre{i}")).selected(self.pre_choisi == i).size([0.0, 52.0]).build() {
                    self.pre_choisi = i;
                }
                let a = ui.item_rect_min();
                let dl = ui.get_window_draw_list();
                dl.add_text([a[0] + 8.0, a[1] + 6.0], charte::c(charte::TEXT), p.nom);
                let petit: String = p.detail.chars().take(44).collect();
                dl.add_text([a[0] + 8.0, a[1] + 28.0], charte::c(charte::TEXT_2), petit);
            }
        });
        ui.same_line();
        let pre = PREREGLAGES[self.pre_choisi];
        ui.child_window("details").size([0.0, haut]).border(true).build(ui, || {
            let ligne = |nom: &str, valeur: String| {
                ui.text_colored(charte::c(charte::TEXT_2), nom);
                ui.same_line_with_pos(130.0);
                ui.text(valeur);
            };
            let v = &self.voix.reglages;
            ui.text("Image");
            ligne("Réglage", pre.detail.to_string());
            ligne("Codeur", pre.x264.map_or("GIF".to_string(), |(crf, vitesse)| format!("H.264 x264 · qualité {crf} · {vitesse}")));
            ui.spacing();
            ui.text("Son");
            if pre.son {
                ligne("Niveau visé", if v.niveau { format!("{:.0} LUFS · crête {:.1} dB", v.niveau_lufs, v.crete_db) } else { "non réglé".to_string() });
                ligne("Voix", if v.filtres().is_some() { "nettoyée".to_string() } else { "telle quelle".to_string() });
            } else {
                ligne("Son", "aucun".to_string());
            }
            if ui.small_button("Nettoyer la voix…") {
                self.voix.ouvrir();
            }
            ui.spacing();
            ui.text("À côté de la vidéo");
            ligne("Vignette en tête", if pre.youtube { "3 s, si elle est écrite".to_string() } else { "non".to_string() });
            ligne("Chapitres", "onglet YouTube".to_string());
            if let Some(d) = pre.duree_max {
                ligne("Durée", format!("{d:.0} s au plus"));
            }
            ui.spacing();
            let sortie = self.prise.as_ref().map_or("ouvre une prise".to_string(), |p| noyau::chemin_derive(&p.video, pre.suffixe).display().to_string());
            ui.text_colored(charte::c(charte::TEXT_2), "Sortie");
            ui.text_wrapped(sortie);
        });
        ui.spacing();
        ui.text_colored(charte::c(charte::TEXT_2), "File d'attente : les rendus tournent en fond, tu continues à monter.");
        let mut retirer = None;
        ui.child_window("file").size([0.0, -40.0]).border(true).build(ui, || {
            if self.file.is_empty() {
                ui.text_colored(charte::c(charte::TEXT_DISABLED), "Rien pour l'instant.");
            }
            for (i, t) in self.file.iter().enumerate() {
                let _id = ui.push_id(i as i32);
                ui.text(&t.nom);
                ui.same_line_with_pos(240.0);
                ui.text_colored(charte::c(charte::TEXT_2), PREREGLAGES[t.pre].nom);
                ui.same_line_with_pos(430.0);
                match t.etat {
                    EtatTravail::Attente => {
                        ui.text_colored(charte::c(charte::TEXT_2), "en attente");
                        ui.same_line();
                        if ui.small_button("Retirer") {
                            retirer = Some(i);
                        }
                    }
                    EtatTravail::EnCours => {
                        let f = self.rendu.as_ref().map_or(0.0, |r| r.suivi.fraction());
                        ui.progress_bar(f as f32).size([200.0, 16.0]).build();
                        ui.same_line();
                        let reste = self.rendu.as_ref().map_or(0.0, |r| r.debut.elapsed().as_secs_f64()) * (1.0 - f) / f.max(0.01);
                        ui.text_colored(charte::c(charte::CHIFFRE_SAISIE), format!("{:.0} % · reste {}", f * 100.0, chrono_court(reste)));
                    }
                    EtatTravail::Fini { reussi: true, secondes } => ui.text_colored(charte::c(charte::SUCCES_TEXTE), format!("fini en {}", chrono_court(secondes))),
                    EtatTravail::Fini { reussi: false, .. } => ui.text_colored(charte::c(charte::ERREUR), "échec ou arrêt"),
                }
            }
        });
        if let Some(i) = retirer {
            self.file.remove(i);
        }
        let pret = self.prise.is_some();
        if ui.button_with_size("Ajouter à la file", [170.0, 30.0]) && pret {
            self.mettre_en_file(self.pre_choisi, false);
        }
        ui.same_line();
        if ui.button_with_size("Rendre maintenant", [170.0, 30.0]) && pret {
            self.mettre_en_file(self.pre_choisi, true);
        }
    }

    /// Lance l'envoi de la prise ouverte, en privé.
    fn envoyer(&mut self) {
        let Some(p) = &self.prise else { return };
        let Some(montee) = youtube::Youtube::a_envoyer(&p.video) else {
            self.message = "Rien à envoyer : il faut d'abord rendre la vidéo, vignette écrite.".into();
            return;
        };
        // Ce qui part est aussi ce qui reste écrit à côté de la vidéo.
        self.youtube.marquer();
        if let Some(e) = self.youtube.ecrire_si_besoin(true) {
            self.message = e;
        }
        match youtube::envoyer(&montee, &p.video, &self.youtube.description) {
            Ok(e) => {
                self.message = format!("Envoi de {}…", montee.file_name().unwrap_or_default().to_string_lossy());
                self.envoi = Some(e);
            }
            Err(e) => self.message = e,
        }
    }

    /// Lance VerdierCAM pour filmer : ses gestes vont où le Monteur les cherchera.
    fn filmer(&mut self) {
        let programme = std::env::var_os("MONTEUR_VERDIERCAM")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join("VerdierCAM/programmes/VerdierCAM.AppImage"));
        if !programme.exists() {
            self.message = format!("VerdierCAM est introuvable : {}", programme.display());
            return;
        }
        let gestes = chemin_fiches().with_file_name("gestes");
        let _ = std::fs::create_dir_all(&gestes);
        let lance = std::process::Command::new(&programme)
            .env("VCAM_JOURNAL_VIDEO", &gestes)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.message = match lance {
            Ok(_) => {
                if self.capture.is_none() {
                    self.capture_prevue = Some(std::time::Instant::now() + DELAI_VERDIERCAM);
                }
                "VerdierCAM lancé, ses gestes sont notés. L'enregistrement va demander sa fenêtre.".into()
            }
            Err(e) => format!("VerdierCAM n'a pas pu être lancé : {e}"),
        };
    }

    /// Le texte de la prise : un clic va à la phrase, « Couper » la retire du montage.
    fn panneau_texte(&mut self, ui: &Ui) {
        let Some(p) = &mut self.prise else {
            ui.text_colored(charte::c(charte::TEXT_2), "Choisir une prise à gauche.");
            return;
        };
        if p.montage.sous_titres.is_empty() {
            ui.text_wrapped("Pas encore de texte : Préparer le montage écoute la voix et l'écrit ici.");
            return;
        }
        ui.set_next_item_width(-1.0);
        ui.input_text("##cherche", &mut self.cherche).hint("Chercher dans le texte…").build();
        let cherche = self.cherche.to_lowercase();
        let t = p.lecteur.position();
        let lecture = p.lecteur.en_lecture();
        let mut aller = None;
        let mut basculer = None;
        ui.child_window("phrases").build(ui, || {
            for (i, s) in p.montage.sous_titres.iter().enumerate() {
                if !cherche.is_empty() && !s.texte.to_lowercase().contains(&cherche) {
                    continue;
                }
                let coupee = p.montage.dans_une_coupe((s.debut + s.fin) / 2.0);
                let courante = s.debut <= t && t < s.fin;
                if courante && lecture && self.phrase_suivie != Some(i) {
                    self.phrase_suivie = Some(i);
                    ui.set_scroll_here_y(0.5);
                }
                let _id = ui.push_id(i as i32);
                if ui.small_button(if coupee { "Rétablir" } else { "Couper" }) {
                    basculer = Some(i);
                }
                ui.same_line();
                let couleur = if coupee { charte::TEXT_DISABLED } else if courante { charte::CHIFFRE_SAISIE } else { charte::TEXT };
                let _encre = ui.push_style_color(StyleColor::Text, charte::c(couleur));
                ui.text_wrapped(format!("{}  {}", etat::temps(s.debut), s.texte));
                if ui.is_item_hovered() {
                    ui.get_window_draw_list().add_rect(ui.item_rect_min(), ui.item_rect_max(), charte::vec(charte::ACCENT, 0.15)).filled(true).build();
                    if ui.is_mouse_clicked(MouseButton::Left) {
                        aller = Some(s.debut);
                    }
                }
            }
        });
        if let Some(a) = aller {
            p.lecteur.aller_a(a);
        }
        if let Some(i) = basculer {
            self.message = p.basculer_phrase(i);
        }
    }

    /// Lance l'enregistrement de l'écran, dans le dossier affiché.
    fn filmer_ecran(&mut self) {
        self.lancer_capture(false);
    }

    /// Une vidéo vite faite, pour montrer un bug : toujours dans Copies d'écran, sans décompte, et le
    /// projet ouvert n'en sait rien.
    fn filmer_simple(&mut self) {
        self.lancer_capture(true);
    }

    fn lancer_capture(&mut self, simple: bool) {
        if self.capture.is_some() {
            return;
        }
        let dossier = if simple { self.copies.clone() } else { self.dossier.chemin.clone() };
        // Un enregistrement simple vise souvent une autre fenêtre que la dernière : on la redemande.
        let jeton = chemin_jeton_portail();
        match capture::Capture::lancer(&dossier, (!simple).then_some(jeton.as_path()), !simple) {
            Ok(c) => {
                self.capture = Some(c);
                self.capture_simple = simple;
                self.message = "Choisis la fenêtre à filmer.".into();
                self.raccourci.get_or_insert_with(raccourci::lancer);
            }
            Err(e) => self.message = e,
        }
    }

    fn suivre_capture(&mut self) {
        while let Some(e) = self.raccourci.as_ref().and_then(|r| r.try_recv().ok()) {
            match e {
                raccourci::Evenement::Pret => eprintln!("raccourci global prêt"),
                raccourci::Evenement::Appui => self.capture.iter_mut().for_each(capture::Capture::arreter),
                // Le bouton Arrêter reste là : un simple mot, sans gêner.
                raccourci::Evenement::Echec(e) => eprintln!("raccourci global indisponible : {e}"),
            }
        }
        if self.capture_prevue.is_some_and(|t| std::time::Instant::now() >= t) {
            self.capture_prevue = None;
            self.filmer_ecran();
        }
        let Some(fin) = self.capture.as_mut().and_then(capture::Capture::suivre) else { return };
        let duree = self.capture.take().and_then(|c| c.duree).unwrap_or(0.0);
        self.message = match fin {
            capture::Fin::Prise(p) if self.capture_simple => {
                if self.dossier.chemin == self.copies {
                    self.a_changer = Some(self.copies.clone());
                }
                format!("Vidéo enregistrée : {} ({}).", p.display(), chrono_court(duree))
            }
            capture::Fin::Prise(p) => {
                self.a_changer = Some(self.dossier.chemin.clone());
                let message = format!("Prise enregistrée : {} ({}). Préparation en cours.", p.file_name().unwrap_or_default().to_string_lossy(), chrono_court(duree));
                self.a_ouvrir = Some(p.clone());
                self.a_preparer = Some(p);
                message
            }
            capture::Fin::Annulee => "Enregistrement annulé.".into(),
            capture::Fin::Erreur(e) => e,
        };
    }

    /// Le dialogue de fichiers de KDE, en fond ; rend le chemin choisi (vide : abandon).
    fn dialogue(depart: PathBuf, filtre: &'static str) -> Receiver<String> {
        let (envoi, recu) = mpsc::channel();
        std::thread::spawn(move || {
            let sortie = std::process::Command::new("kdialog").arg("--getopenfilename").arg(&depart).arg(filtre).output();
            let _ = envoi.send(sortie.map(|s| String::from_utf8_lossy(&s.stdout).trim().to_string()).unwrap_or_default());
        });
        recu
    }

    /// Ouvre un fichier `.monteur` : la vidéo, à son chemin ou retrouvée à côté du fichier.
    fn ouvrir_fichier_projet(&mut self, chemin: PathBuf) {
        match noyau::projet::Projet::lire(&chemin) {
            Ok(projet) => match projet.retrouver_video(&chemin, &[self.copies.clone()]) {
                Some(video) => {
                    let montage = etat::chemin_montage(&video);
                    if !montage.exists()
                        && let Some(mut m) = projet.montage()
                    {
                        let _ = m.ecrire(&montage);
                    }
                    self.changer_de_dossier(video.parent().unwrap_or(&self.copies).to_path_buf());
                    self.a_ouvrir = Some(video);
                }
                None => self.message = format!("Sa vidéo est introuvable : {}", projet.video.display()),
            },
            Err(e) => self.message = e.0,
        }
    }

    /// Choisit une vidéo (dialogue de KDE), la copie dans le projet et la pose à la tête.
    fn inserer_une_video(&mut self, par_dessus: bool) {
        let Some(p) = &self.prise else { return };
        if !p.dossier().starts_with(&self.racine) {
            self.message = "Une vidéo se range avec son projet : fais d'abord « Nouveau projet ».".into();
            return;
        }
        let (dossier, t) = (p.dossier().to_path_buf(), p.lecteur.position());
        let depart = std::env::home_dir().unwrap_or_default().join("Vidéos");
        let (envoi, recu) = mpsc::channel();
        std::thread::spawn(move || {
            let choix = std::process::Command::new("kdialog")
                .arg("--getopenfilename")
                .arg(&depart)
                .arg("Vidéos (*.mp4 *.mkv *.mov *.MP4 *.MOV)")
                .output();
            let chemin = choix.map(|s| String::from_utf8_lossy(&s.stdout).trim().to_string()).unwrap_or_default();
            let _ = envoi.send(if chemin.is_empty() { Ok(None) } else { importer(&PathBuf::from(chemin), &dossier, par_dessus, t).map(Some) });
        });
        self.import = Some(recu);
        self.message = "Choisis la vidéo à insérer…".into();
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
                let fiche = self.fiches.fiches.entry(noyau::videos::cle(&video)).or_default().clone();
                self.vignette.charger(&video, &fiche, p.infos.duree);
                self.youtube.charger(&video);
                self.bande_neuve = true;
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
        if p.coupe_commencee.is_some() && (ui.is_key_pressed(Key::Enter) || ui.is_key_pressed(Key::KeypadEnter)) {
            let t = p.lecteur.position();
            self.message = p.marquer_coupe(t);
        }
        let (ctrl, maj) = (ui.io().key_ctrl(), ui.io().key_shift());
        let image = 1.0 / 30.0;
        let t = p.lecteur.position();
        if let Some(ic) = &mut p.intercalee {
            // Une vidéo intercalée passe : Espace la met en pause, Échap revient à la prise.
            if ui.is_key_pressed(Key::Space) {
                ic.lecteur.basculer();
            }
            if ui.is_key_pressed(Key::Escape) {
                p.sortir_de_l_intercalee();
            }
            return;
        }
        if ui.is_key_pressed(Key::Space) {
            p.lecteur.basculer();
        }
        if ui.is_key_pressed(Key::K) {
            p.lecteur.arreter();
        }
        if ui.is_key_pressed(Key::S) && !ctrl {
            self.message = p.scinder(t);
        }
        if ui.is_key_pressed(Key::M) && !ctrl {
            p.poser_chapitre(t);
            self.message = "Chapitre posé à la tête : donne-lui son titre dans l'inspecteur.".into();
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
        if ctrl && ui.is_key_pressed(Key::S) {
            self.enregistrer();
        }
    }

    fn panneau_prises(&mut self, ui: &Ui, textures: &[Option<TextureId>]) {
        // Où l'on est : les prises d'OBS, ou un projet.
        let mut noms = vec!["Copies d'écran".to_string()];
        noms.extend(self.projets.iter().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()));
        let mut rang = self.projets.iter().position(|p| *p == self.dossier.chemin).map_or(0, |i| i + 1);
        let largeur = ui.push_item_width(-1.0);
        if ui.combo_simple_string("##lieu", &mut rang, &noms) {
            let chemin = if rang == 0 { self.copies.clone() } else { self.projets[rang - 1].clone() };
            self.a_changer = Some(chemin);
        }
        drop(largeur);
        // Où c'est sur le disque : le projet est un dossier, on doit pouvoir le retrouver.
        let maison = std::env::home_dir().unwrap_or_default();
        let lieu = self.dossier.chemin.strip_prefix(&maison).map_or(self.dossier.chemin.display().to_string(), |r| format!("~/{}", r.display()));
        {
            let _gris = ui.push_style_color(StyleColor::Text, charte::c(charte::UNIT));
            ui.text_wrapped(&lieu);
        }
        if ui.small_button("Ouvrir le dossier") {
            self.ouvrir_le_dossier();
        }
        if self.dossier.chemin.starts_with(&self.racine) {
            ui.same_line();
            if ui.small_button("Fermer le projet") {
                self.fermer_projet();
            }
            if ui.is_item_hovered() {
                ui.tooltip_text("Tout est déjà enregistré. Retour à Copies d'écran : ce que tu filmes ensuite n'ira pas dans ce projet.");
            }
        }
        ui.spacing();
        let ouverte = self.prise.as_ref().map(|p| p.video.clone());
        const HAUT: f32 = 58.0;
        let (mh, ml) = (46.0, 46.0 * miniatures::LARGEUR as f32 / miniatures::HAUTEUR as f32);
        let mut demande = None;
        for (i, (chemin, etat)) in self.dossier.prises.iter().enumerate() {
            if ui.selectable_config(format!("##prise{i}")).selected(ouverte.as_ref() == Some(chemin)).size([0.0, HAUT]).build() {
                self.a_ouvrir = Some(chemin.clone());
            }
            if ui.is_item_clicked_with_button(MouseButton::Right) {
                demande = Some(chemin.clone());
            }
            if ui.is_item_hovered() {
                ui.tooltip_text("Clic droit : mettre à la corbeille");
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
        if let Some(d) = demande {
            self.demander_corbeille(d);
        }
    }

    /// Sous la barre de menus : ce qu'on fait le plus souvent, en vue.
    fn ruban(&mut self, ui: &Ui) {
        use outils::{Allure, Icone, filet, outil};
        let t = charte::TEXT;
        let pret = self.prise.as_ref().is_some_and(|p| p.preparation.is_none());
        if outil(ui, "##preparer", Icone::Preparer, "Préparer le montage", t, Allure::Primaire, "Zooms, étiquettes, sous-titres, temps morts : tout d'un coup.")
            && pret
            && let Some(p) = &mut self.prise
        {
            p.preparer();
            self.message = "Préparation en cours : voix, mouvement de l'image, gestes, silences…".into();
        }
        filet(ui);
        if outil(ui, "##rendre", Icone::Rendre, "Rendre", t, Allure::Simple, "Préréglages et file d'attente.") {
            self.fenetre_rendre = true;
        }
        ui.same_line();
        if outil(ui, "##voix", Icone::Voix, "Voix", charte::SUCCES, Allure::Simple, "Nettoyer la voix, écouter avant et après.") {
            self.voix.ouvrir();
        }
        filet(ui);
        if let Some(c) = &mut self.capture {
            let texte = match (c.temps(), c.decompte_restant()) {
                _ if c.en_finition() => "Finition du son…".to_string(),
                _ if c.en_arret() => "Arrêt en cours… (cliquer force)".to_string(),
                (_, Some(n)) => format!("Départ dans {n}"),
                (Some(t), _) => format!("Arrêter {}", chrono_court(t)),
                _ => "Annuler le choix".to_string(),
            };
            if outil(ui, "##arret_capture", Icone::Filmer, &texte, charte::ERREUR, Allure::EnCours, "") {
                c.arreter();
            }
            if let Some(db) = c.niveau().filter(|_| !c.en_finition()) {
                ui.same_line();
                vu_metre(ui, db, c.micro_muet());
            }
        } else {
            if outil(ui, "##filmer", Icone::Filmer, "Filmer", charte::ERREUR, Allure::Simple, "Filme une fenêtre, sans OBS. La prise arrive dans la liste. Fichier › Filmer une autre fenêtre pour en changer.") {
                self.capture_prevue = None;
                self.filmer_ecran();
            }
            ui.same_line();
            if outil(ui, "##simple", Icone::Filmer, "Enregistrement simple", charte::ALERTE, Allure::Simple, "Choisis la fenêtre à chaque fois. Toujours dans Copies d'écran, sans décompte ni préparation : le projet ouvert ne bouge pas.") {
                self.capture_prevue = None;
                self.filmer_simple();
            }
            ui.same_line();
            if outil(ui, "##verdier", Icone::Verdier, "Filmer VerdierCAM", t, Allure::Simple, "Lance VerdierCAM et l'enregistrement : ses gestes sont notés pour préparer le montage.") {
                self.filmer();
            }
        }
        let Some(p) = &mut self.prise else { return };
        filet(ui);
        if outil(ui, "##annuler", Icone::Annuler, "Annuler", t, Allure::Simple, "Ctrl+Z") {
            p.defaire();
        }
        ui.same_line();
        if outil(ui, "##retablir", Icone::Retablir, "Rétablir", t, Allure::Simple, "Ctrl+Maj+Z") {
            p.refaire();
        }
    }

    /// Les gestes de montage, au-dessus de la ligne de temps : là où on les fait.
    fn barre_montage(&mut self, ui: &Ui) {
        use outils::{Allure, Icone, filet, outil};
        let c = charte::TEXT;
        let Some(p) = &mut self.prise else { return };
        let t = p.lecteur.position();
        let en_cours = p.coupe_commencee.is_some();
        let (icone, nom, allure) = if en_cours { (Icone::Valider, "Valider la coupe", Allure::EnCours) } else { (Icone::Couper, "Couper (C)", Allure::Simple) };
        if outil(ui, "##couper", icone, nom, c, allure, "Une première fois au début du passage, puis clic droit, Entrée ou C à sa fin. Échap abandonne.") {
            self.message = p.marquer_coupe(t);
        }
        ui.same_line();
        if outil(ui, "##scinder", Icone::Scinder, "Scinder (S)", c, Allure::Simple, "Coupe en deux l'élément choisi, à la tête de lecture.") {
            self.message = p.scinder(t);
        }
        ui.same_line();
        if outil(ui, "##avant", Icone::CouperAvant, "Couper avant", c, Allure::Simple, "Retire tout ce qui précède la tête de lecture.") {
            p.ajouter_coupe(0.0, t);
        }
        ui.same_line();
        if outil(ui, "##apres", Icone::CouperApres, "Couper après", c, Allure::Simple, "Retire tout ce qui suit la tête de lecture.") {
            p.ajouter_coupe(t, p.montage.duree);
        }
        filet(ui);
        if outil(ui, "##zoom", Icone::Zoom, "Zoom (Z)", charte::PISTE_ZOOM, Allure::Simple, "Un zoom à la tête de lecture : trace son cadre dans la visionneuse.") {
            p.nouveau_zoom(t);
            self.message = "Zoom posé : trace son cadre dans la visionneuse.".into();
        }
        ui.same_line();
        if outil(ui, "##chapitre", Icone::Chapitre, "Chapitre (M)", charte::MARQUEUR, Allure::Simple, "Un chapitre à la tête de lecture.") {
            p.poser_chapitre(t);
            self.message = "Chapitre posé à la tête : donne-lui son titre dans l'inspecteur.".into();
        }
        filet(ui);
        if outil(ui, "##intercaler", Icone::Intercaler, "Intercaler", c, Allure::Simple, "À la tête de lecture : la prise s'arrête, la vidéo passe avec son son, la prise reprend.") {
            self.inserer_une_video(false);
        }
        ui.same_line();
        if outil(ui, "##dessus", Icone::Dessus, "Par-dessus", c, Allure::Simple, "À la tête de lecture : la vidéo remplace l'image, ta voix continue.") {
            self.inserer_une_video(true);
        }
    }

    /// La boîte « Nouveau projet ».
    fn demander_corbeille(&mut self, video: PathBuf) {
        self.a_jeter = Some(video);
        self.ouvrir_corbeille = true;
    }

    /// La confirmation : le nom de la prise, et tout ce qui part avec elle.
    fn boite_corbeille(&mut self, ui: &Ui) {
        const TITRE: &str = "Mettre à la corbeille";
        if std::mem::take(&mut self.ouvrir_corbeille) {
            ui.open_popup(TITRE);
        }
        let Some(boite) = ui.begin_modal_popup_config(TITRE).flags(WindowFlags::ALWAYS_AUTO_RESIZE).begin() else { return };
        let Some(video) = self.a_jeter.clone() else {
            ui.close_current_popup();
            boite.end();
            return;
        };
        let fichiers = noyau::videos::fichiers_lies(&video);
        let octets: u64 = fichiers.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
        ui.text(video.file_name().unwrap_or_default().to_string_lossy());
        ui.text_colored(charte::c(charte::TEXT_2), format!("{} fichier(s), {:.0} Mo :", fichiers.len(), octets as f64 / 1e6));
        for f in &fichiers {
            ui.bullet_text(f.file_name().unwrap_or_default().to_string_lossy());
        }
        ui.spacing();
        ui.text_wrapped("Tu pourras les récupérer depuis la corbeille.");
        if !self.fiches.fiches.get(&noyau::videos::cle(&video)).map_or("", |f| f.youtube_id.as_str()).is_empty() {
            ui.text_colored(charte::c(charte::ALERTE), "Cette vidéo a été envoyée : elle reste sur YouTube.");
        }
        ui.spacing();
        if ui.button_with_size("Mettre à la corbeille", [180.0, 30.0]) {
            self.a_jeter = None;
            self.mettre_a_la_corbeille(&video);
            ui.close_current_popup();
        }
        ui.same_line();
        if ui.button_with_size("Annuler##corbeille", [100.0, 30.0]) {
            self.a_jeter = None;
            ui.close_current_popup();
        }
        boite.end();
    }

    /// Jamais de suppression définitive : `gio trash`, récupérable dans la corbeille de KDE.
    fn mettre_a_la_corbeille(&mut self, video: &Path) {
        if self.rendu.is_some() || self.envoi.is_some() {
            self.message = "Un rendu ou un envoi est en cours : attends sa fin.".into();
            return;
        }
        if self.prise.as_ref().is_some_and(|p| p.video == video) {
            if let Some(e) = self.prise.as_mut().and_then(|p| p.ecrire_si_besoin(true)) {
                self.message = e;
                return;
            }
            // Le verrou et le fichier ouvert partent avec la prise.
            self.prise = None;
            self.vue.oublier_source();
        }
        let fichiers = noyau::videos::fichiers_lies(video);
        let sortie = std::process::Command::new("gio").arg("trash").arg("--").args(&fichiers).stdin(std::process::Stdio::null()).output();
        self.a_changer = Some(self.dossier.chemin.clone());
        self.message = match sortie {
            Ok(s) if s.status.success() => format!("{} fichier(s) à la corbeille : {}.", fichiers.len(), video.file_name().unwrap_or_default().to_string_lossy()),
            Ok(s) => format!("Corbeille impossible : {}", String::from_utf8_lossy(&s.stderr).trim()),
            Err(e) => format!("Corbeille impossible : {e}"),
        };
    }

    fn boite_projet(&mut self, ui: &Ui) {
        if std::mem::take(&mut self.ouvrir_boite) {
            ui.open_popup("Nouveau projet");
        }
        if let Some(boite) = ui.begin_modal_popup("Nouveau projet") {
            ui.text_wrapped("Un projet est un dossier : la vidéo, sa vignette, son montage et ses rendus y sont rangés ensemble.");
            ui.spacing();
            ui.text("Le nom du projet :");
            let l = ui.push_item_width(380.0);
            let entree = ui.input_text("##nom", &mut self.nom_projet).enter_returns_true(true).build();
            drop(l);
            let part = self.prise.as_ref().is_some_and(|p| !p.dossier().starts_with(&self.racine));
            ui.text_colored(
                charte::c(charte::UNIT),
                if part { format!("La prise et tout ce qui l'accompagne partent dans {}/", self.racine.display()) } else { format!("Un dossier vide dans {}/", self.racine.display()) },
            );
            let valide = noyau::projets::nom_de_projet(&self.nom_projet).is_some();
            if (ui.button_with_size("Créer le projet", [180.0, 30.0]) || entree) && valide {
                let nom = self.nom_projet.clone();
                ui.close_current_popup();
                self.nouveau_projet(&nom);
            }
            ui.same_line();
            if ui.button_with_size("Annuler##projet", [100.0, 30.0]) {
                ui.close_current_popup();
            }
            boite.end();
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
                    // Haut gauche et bas droit : « \\ » ; haut droit et bas gauche : « / ».
                    match proche {
                        Some(0 | 3) => ui.set_mouse_cursor(Some(MouseCursor::ResizeNWSE)),
                        Some(_) => ui.set_mouse_cursor(Some(MouseCursor::ResizeNESW)),
                        None => {}
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
            match &mut p.intercalee {
                Some(ic) => ic.lecteur.basculer(),
                None => p.lecteur.basculer(),
            }
        }
        ui.same_line();
        if bouton_forme(ui, "stop", Forme::Stop, false, "Stop : revient d'où la lecture est partie") {
            if p.intercalee.is_some() { p.sortir_de_l_intercalee() } else { p.lecteur.stop() }
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
            Some(Choix { sorte: Sorte::Insertion, indice }) if indice < p.montage.insertions.len() => {
                titre(ui, "Vidéo insérée");
                let i = &mut p.montage.insertions[indice];
                ui.text_wrapped(i.video.rsplit('/').next().unwrap_or(&i.video));
                let modes = ["intercalée", "par-dessus"];
                let mut m = usize::from(i.par_dessus());
                if ui.combo_simple_string("Pose", &mut m, &modes) {
                    i.mode = if m == 1 { "dessus" } else { "intercalee" }.into();
                    change = true;
                }
                change |= nombre(ui, "Arrive à (s)", &mut i.t, 0.1);
                change |= nombre(ui, "Début gardé (s)", &mut i.debut, 0.1);
                change |= nombre(ui, "Fin gardée (s)", &mut i.fin, 0.1);
                ui.text_colored(charte::c(charte::UNIT), format!("durée {}", temps(i.duree())));
            }
            Some(Choix { sorte: Sorte::Chapitre, indice }) if indice < p.montage.chapitres.len() => {
                titre(ui, "Chapitre");
                let c = &mut p.montage.chapitres[indice];
                change |= ui.input_text("Titre", &mut c.titre).build();
                change |= nombre(ui, "Instant (s)", &mut c.t, 0.1);
                ui.text_wrapped("Le premier chapitre sera à 0:00 sur YouTube. Il en faut 3 au moins, à 10 s l'un de l'autre.");
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
                let mut fondu = r.fondu as f32;
                ui.set_next_item_width(120.0);
                if ui.slider_f32("Fondu aux coupes (s)", &mut fondu, 0.0, 1.0) {
                    r.fondu = (fondu as f64 * 10.0).round() / 10.0;
                    change = true;
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Image et son se fondent à chaque coupe, au rendu. 0 : coupes franches.");
                }
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
        self.vignette.liberer(g.gpu());
        self.bande.liberer(g.gpu());
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
        self.suivre_capture();
        // La liste suit le dossier toute seule : une prise d'OBS y apparaît en quelques secondes.
        if self.regarde.elapsed().as_secs() >= 2 {
            self.regarde = std::time::Instant::now();
            // La même lecture que la liste (sans les ignorées) : sinon elle se referait sans fin.
            if Dossier::lire(&self.dossier.chemin, &self.fiches).prises != self.dossier.prises {
                self.a_changer = Some(self.dossier.chemin.clone());
            }
        }
        if std::mem::take(&mut self.youtube.demande) && self.envoi.is_none() {
            self.envoyer();
        }
        if let Some(e) = self.youtube.ecrire_si_besoin(false) {
            self.message = e;
        }
        if let Some(fin) = self.envoi.as_mut().and_then(|e| e.suivre()) {
            self.envoi = None;
            self.message = fin.message;
            if let Some(id) = fin.youtube_id {
                let f = self.fiches.fiches.entry(noyau::videos::cle(&fin.video)).or_default();
                f.youtube_id = id;
                f.envoyee_le = noyau::maintenant();
                self.ecrire_fiches();
                self.a_changer = Some(self.dossier.chemin.clone());
            }
        }
        if let Some(fini) = self.rendu.as_ref().and_then(|r| r.fin.try_recv().ok()) {
            if let Some(r) = self.rendu.take() {
                let duree = r.debut.elapsed().as_secs_f64();
                let _ = r.fil.join();
                if let Some(t) = self.file.iter_mut().find(|t| matches!(t.etat, EtatTravail::EnCours)) {
                    t.etat = EtatTravail::Fini { reussi: fini.is_ok(), secondes: duree };
                }
                self.message = match fini {
                    Ok(compte) => format!("{compte} En {:.0} s.", duree),
                    Err(e) => e,
                };
            }
            self.a_changer = Some(self.dossier.chemin.clone());
            self.lancer_suivant();
        }
        if let Some(r) = &self.prise_choisie
            && let Ok(chemin) = r.try_recv()
        {
            self.prise_choisie = None;
            if !chemin.is_empty() {
                match noyau::videos::deplacer(&mut self.fiches, &chemin_fiches(), &PathBuf::from(&chemin), &self.dossier.chemin.clone()) {
                    Ok(v) => {
                        self.a_changer = Some(self.dossier.chemin.clone());
                        self.a_ouvrir = Some(v);
                    }
                    Err(e) => self.message = e.0,
                }
            }
        }
        if let Some(r) = &self.projet_choisi
            && let Ok(chemin) = r.try_recv()
        {
            self.projet_choisi = None;
            if !chemin.is_empty() {
                self.ouvrir_fichier_projet(PathBuf::from(chemin));
            }
        }
        if let Some(d) = self.a_changer.take() {
            self.changer_de_dossier(d);
        }
        if let Some(v) = self.a_ouvrir.take() {
            if self.fiches_sales {
                self.ecrire_fiches();
            }
            self.ouvrir(v);
            if let (Some(f), Some(p)) = (self.a_preparer.take(), &mut self.prise)
                && p.video == f
            {
                p.preparer();
            }
        }
        if let Some(r) = &self.import
            && let Ok(resultat) = r.try_recv()
        {
            self.import = None;
            match resultat {
                Ok(Some((video, duree, par_dessus, t))) => {
                    if let Some(p) = &mut self.prise {
                        p.ajouter_insertion(video, duree, par_dessus, t);
                    }
                    self.message = "Vidéo posée : elle se règle dans l'inspecteur.".into();
                }
                Ok(None) => self.message = "Aucune vidéo choisie.".into(),
                Err(e) => self.message = e,
            }
        }
        if let Some(p) = &mut self.prise {
            if let Some(m) = p.suivre_le_fond() {
                self.message = m;
            }
            if let Some(m) = p.suivre_les_insertions() {
                self.message = m;
            }
            // En lecture, les passages coupés se sautent et les accélérés passent accélérés, comme
            // au rendu : l'image va plus vite, le son reste à sa vitesse.
            if p.lecteur.en_lecture() {
                let t = p.lecteur.position();
                if let Some((_, b)) = p.montage.coupes_actives().into_iter().find(|&(a, b)| a <= t && t < b - 0.05) {
                    p.lecteur.aller_a(b);
                } else {
                    match p.montage.vitesse_a(t) {
                        v if v > 1.0 && !p.lecteur.en_passage() => p.lecteur.commencer_passage(v),
                        v if v <= 1.0 && p.lecteur.en_passage() => p.lecteur.aller_a(t),
                        _ => {}
                    }
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
        // Au début du tour, avant tout dessin : ce tour-ci ne doit plus citer les anciennes textures
        // (libérées à la fin d'un tour, « Invalid texture ID » arrêtait tout).
        if std::mem::take(&mut self.relire) {
            self.miniatures.liberer(context.gpu());
            self.miniatures = miniatures::Miniatures::lancer(self.dossier.prises.iter().map(|p| p.0.clone()).collect());
        }
        if std::mem::take(&mut self.bande_neuve) {
            self.bande.liberer(context.gpu());
            self.bande = match &self.prise {
                Some(p) => miniatures::Bande::lancer(p.video.clone(), p.infos.duree),
                None => miniatures::Bande::vide(),
            };
        }
        let bande = self.bande.preparer(context.gpu());
        self.miniatures.recevoir(context.gpu());
        let textures = self.miniatures.textures(context.gpu());
        let cle_fiche = self.prise.as_ref().map(|p| noyau::videos::cle(&p.video));
        let fiche = cle_fiche.as_ref().map(|c| self.fiches.fiches.entry(c.clone()).or_default());
        let apercu = self.vignette.preparer(context.gpu(), fiche, &self.polices);
        let ui = context.ui();
        self.touches(ui);

        if let Some(barre) = ui.begin_main_menu_bar() {
            if let Some(m) = ui.begin_menu("Fichier") {
                if ui.menu_item("Nouveau projet…") {
                    let cle = self.prise.as_ref().map(|p| noyau::videos::cle(&p.video)).unwrap_or_default();
                    self.nom_projet = self.fiches.fiches.get(&cle).map(|f| f.titre.clone()).unwrap_or_default();
                    self.ouvrir_boite = true;
                }
                if let Some(o) = ui.begin_menu("Ouvrir un projet") {
                    if ui.menu_item("Copies d'écran") {
                        self.a_changer = Some(self.copies.clone());
                    }
                    ui.separator();
                    for p in self.projets.clone() {
                        if ui.menu_item(p.file_name().unwrap_or_default().to_string_lossy()) {
                            self.a_changer = Some(p);
                        }
                    }
                    ui.separator();
                    if ui.menu_item("Un fichier .monteur…") {
                        self.projet_choisi = Some(Self::dialogue(self.racine.clone(), "Projets du Monteur (*.monteur)"));
                    }
                    o.end();
                }
                let dans_projet = self.dossier.chemin.starts_with(&self.racine);
                if ui.menu_item_enabled_selected_no_shortcut("Ajouter une prise au projet…", false, dans_projet) {
                    self.prise_choisie = Some(Self::dialogue(self.copies.clone(), "Vidéos (*.mp4 *.mkv *.mov)"));
                }
                if ui.menu_item_enabled_selected_no_shortcut("Fermer le projet", false, dans_projet) {
                    self.fermer_projet();
                }
                if ui.menu_item("Ouvrir le dossier sur le disque") {
                    self.ouvrir_le_dossier();
                }
                ui.separator();
                if ui.menu_item_enabled_selected_with_shortcut("Enregistrer le projet", "Ctrl+S", false, self.prise.is_some()) {
                    self.enregistrer();
                }
                if ui.menu_item("Rendre…") {
                    self.fenetre_rendre = true;
                }
                ui.separator();
                let jeton = chemin_jeton_portail();
                if ui.menu_item_enabled_selected_no_shortcut("Filmer une autre fenêtre…", false, self.capture.is_none()) {
                    let _ = std::fs::remove_file(&jeton);
                    self.filmer_ecran();
                }
                if ui.menu_item_enabled_selected_no_shortcut("Mettre la prise à la corbeille…", false, self.prise.is_some())
                    && let Some(p) = &self.prise
                {
                    self.demander_corbeille(p.video.clone());
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
                if ui.menu_item("Nettoyer la voix…") {
                    self.voix.ouvrir();
                }
                if ui.menu_item_enabled_selected_no_shortcut("Préparer le montage", false, pret)
                    && let Some(p) = &mut self.prise
                {
                    p.preparer();
                    self.message = "Préparation en cours : voix, mouvement de l'image, gestes, silences…".into();
                }
                if ui.menu_item_enabled_selected_no_shortcut("Proposer les chapitres", false, pret)
                    && let Some(p) = &mut self.prise
                {
                    let dossiers = [p.dossier().to_path_buf(), chemin_fiches().with_file_name("gestes")];
                    self.message = p.proposer_chapitres(&dossiers);
                }
                if ui.menu_item_enabled_selected_no_shortcut("Refaire les sous-titres", false, pret)
                    && let Some(p) = &mut self.prise
                {
                    p.refaire_les_sous_titres();
                    self.message = "Transcription de la voix (whisper, sur ce poste)…".into();
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
        ui.window(&panneaux.vignette).build(|| {
            let Some(p) = &self.prise else {
                ui.text_colored(charte::c(charte::TEXT_2), "Choisir une prise à gauche.");
                return;
            };
            let (video, t, duree) = (p.video.clone(), p.lecteur.position(), p.infos.duree);
            let fiche = self.fiches.fiches.entry(noyau::videos::cle(&video)).or_default();
            if self.vignette.dessiner(ui, apercu, fiche, &video, t, duree, &self.polices, &mut self.message) {
                self.fiches_sales = true;
            }
        });
        ui.window(&panneaux.inspecteur).build(|| self.panneau_inspecteur(ui));
        ui.window(&panneaux.texte).build(|| self.panneau_texte(ui));
        ui.window(&panneaux.ligne).build(|| {
            self.barre_montage(ui);
            if let Some(p) = &mut self.prise {
                self.ligne.dessiner(ui, p, (&self.bande, bande));
                // Le clic droit valide une coupe commencée, à la tête de lecture.
                if p.coupe_commencee.is_some() && ui.is_window_hovered() && ui.is_mouse_clicked(MouseButton::Right) {
                    let t = p.lecteur.position();
                    self.message = p.marquer_coupe(t);
                }
            }
        });

        if std::mem::take(&mut self.montrer_vignette) {
            ui.set_window_focus(Some("Vignette###vignette"));
        }
        if std::mem::take(&mut self.montrer_texte) {
            ui.set_window_focus(Some("Texte###texte"));
        }
        if std::mem::take(&mut self.montrer_youtube) {
            ui.set_window_focus(Some("YouTube###youtube"));
        }
        ui.window(&panneaux.youtube).build(|| {
            let Some(p) = &self.prise else {
                ui.text_colored(charte::c(charte::TEXT_2), "Choisir une prise à gauche.");
                return;
            };
            let video = p.video.clone();
            let fiche = self.fiches.fiches.entry(noyau::videos::cle(&video)).or_default().clone();
            self.youtube.dessiner(ui, &fiche, &video, &p.montage, self.envoi.as_ref());
        });
        // Hors de l'onglet : il n'est dessiné que s'il est devant.
        if std::mem::take(&mut self.vignette.ecrite) {
            self.ecrire_fiches();
            self.a_changer = Some(self.dossier.chemin.clone());
        }
        self.boite_projet(ui);
        self.boite_corbeille(ui);
        if self.fenetre_rendre {
            let mut ouverte = true;
            ui.window("Rendre le montage###rendre").opened(&mut ouverte).size([940.0, 640.0], Condition::FirstUseEver).build(|| self.panneau_rendre(ui));
            self.fenetre_rendre = ouverte;
        }
        let (video, t) = self.prise.as_ref().map_or((None, 0.0), |p| (Some(p.video.clone()), p.lecteur.position()));
        if self.voix.dessiner(ui, video.as_deref(), t, &chemin_voix(), &mut self.message)
            && let Some(p) = &mut self.prise
        {
            p.lecteur.arreter();
        }

        // La barre d'état.
        ui.window("etat")
            .flags(WindowFlags::NO_DECORATION | WindowFlags::NO_MOVE | WindowFlags::NO_DOCKING | WindowFlags::NO_SAVED_SETTINGS)
            .position([pos[0], pos[1] + taille[1]], Condition::Always)
            .size([taille[0], 24.0], Condition::Always)
            .build(|| {
                ui.text_colored(charte::c(charte::TEXT_2), &self.message);
                if let Some(r) = &self.rendu {
                    ui.same_line();
                    let etape = if r.suivi.vignette.load(std::sync::atomic::Ordering::Relaxed) {
                        "  vignette en tête…".to_string()
                    } else {
                        format!("  rendu de {} : {:.0} %", r.nom, r.suivi.fraction() * 100.0)
                    };
                    ui.text_colored(charte::c(charte::ACCENT_HOVER), etape);
                    ui.same_line();
                    if ui.small_button("Arrêter le rendu") {
                        r.suivi.arret.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                if let Some(p) = &self.prise {
                    if let Some(prep) = &p.preparation {
                        ui.same_line();
                        ui.text_colored(charte::c(charte::ACCENT_HOVER), format!("  préparation… {:.0} s", prep.debut.elapsed().as_secs_f64()));
                    }
                    ui.same_line_with_pos(taille[0] - 360.0);
                    if p.enregistre() {
                        ui.text_colored(charte::c(charte::SUCCES_TEXTE), "● enregistré");
                    } else {
                        ui.text_colored(charte::c(charte::ALERTE), "● à écrire");
                    }
                    ui.same_line_with_pos(taille[0] - 260.0);
                    ui.text_colored(charte::c(charte::UNIT), format!("image peinte en {:.1} ms · {:.0} i/s", self.vue.dernier_ms, ui.io().framerate()));
                }
            });
        Ok(())
    }

    fn shutdown(&mut self, _c: &mut dear_app::ShutdownContext<'_>) -> Result<(), RunError> {
        if let Some(e) = self.youtube.ecrire_si_besoin(true) {
            eprintln!("{e}");
        }
        // Un rendu en cours s'arrête proprement : pas de ffmpeg orphelin, pas de fichier partiel.
        if let Some(r) = self.rendu.take() {
            r.suivi.arret.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = r.fil.join();
        }
        if self.fiches_sales {
            self.ecrire_fiches();
        }
        if let Some(p) = &mut self.prise
            && let Some(e) = p.ecrire_si_besoin(true)
        {
            eprintln!("{e}");
        }
        Ok(())
    }
}

/// Copie une vidéo dans le projet et lit sa durée (dans un fil : une vidéo d'atelier pèse lourd).
fn importer(source: &std::path::Path, dossier: &std::path::Path, par_dessus: bool, t: f64) -> Result<(String, f64, bool, f64), String> {
    let relatif = noyau::projets::importer(source, dossier).map_err(|e| e.0)?;
    let duree = etat::sonder_duree(&dossier.join(&relatif)).ok_or_else(|| format!("{} n'est pas une vidéo lisible", source.display()))?;
    Ok((relatif, duree, par_dessus, t))
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
