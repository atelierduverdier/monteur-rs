//! L'onglet YouTube : le titre, la description, les mots-clés, et l'envoi par `youtube-envoi`.
//! Toujours en privé : la mise en public reste un geste fait dans YouTube Studio. La description
//! s'écrit dans `<vidéo> description YouTube.txt`, le fichier déjà en usage.
//!
//! Sans `youtube-envoi` (tout autre poste que celui de Christophe : il passe par son compte
//! Google), « Préparer pour YouTube » : tout est prêt à côté de la prise, les textes se copient,
//! YouTube Studio s'ouvre, on y glisse la vidéo.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use dear_imgui_rs::Ui;
use noyau::description::{Description, description_proposee};
use noyau::fiches::Fiche;
use noyau::videos;

enum Nouvelle {
    Ligne(String),
    Fin(bool),
}

/// Un envoi en cours. Il survit au changement de prise : la vidéo envoyée est notée ici.
pub struct Envoi {
    pub video: PathBuf,
    nouvelles: Receiver<Nouvelle>,
    pub pourcent: f64,
    sortie: String,
    description: PathBuf,
}

/// Le résultat d'un envoi fini, pour la fiche et le message.
pub struct FinEnvoi {
    pub video: PathBuf,
    pub youtube_id: Option<String>,
    pub message: String,
}

impl Envoi {
    /// Fait avancer l'envoi ; rend son résultat quand il est fini.
    pub fn suivre(&mut self) -> Option<FinEnvoi> {
        while let Ok(n) = self.nouvelles.try_recv() {
            match n {
                Nouvelle::Ligne(l) => {
                    if l.trim_end().ends_with('%')
                        && let Some(p) = videos::lire_pourcentage(&l)
                    {
                        self.pourcent = p;
                    }
                    self.sortie.push_str(&l);
                    self.sortie.push('\n');
                }
                Nouvelle::Fin(reussi) => {
                    let _ = std::fs::remove_file(&self.description);
                    let id = videos::lire_id_youtube(&self.sortie);
                    let srt = noyau::chemin_derive(&self.video, noyau::SUFFIXE_SRT);
                    let mut message = match (&id, reussi) {
                        (Some(i), _) => format!("Envoyée en privé : https://youtu.be/{i}"),
                        (None, _) => format!("Envoi en échec : {}", self.sortie.lines().filter(|l| !l.trim().is_empty()).next_back().unwrap_or("youtube-envoi")),
                    };
                    if self.sortie.contains("Vignette refusée") {
                        message.push_str(" ; vignette refusée : la poser à la main dans Studio");
                    }
                    if id.is_some() && srt.exists() {
                        message.push_str(" ; sous-titres à importer dans Studio");
                    }
                    return Some(FinEnvoi { video: self.video.clone(), youtube_id: id, message });
                }
            }
        }
        None
    }
}

/// Lit une sortie du programme, ligne à ligne ; `\r` compte aussi (l'avancement réécrit sa ligne).
fn suivre_flux(mut flux: impl Read + Send + 'static, envoi: mpsc::Sender<Nouvelle>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (mut courante, mut bloc) = (Vec::new(), [0u8; 512]);
        while let Ok(n) = flux.read(&mut bloc) {
            if n == 0 {
                break;
            }
            for &o in &bloc[..n] {
                if o == b'\n' || o == b'\r' {
                    if !courante.is_empty() {
                        let _ = envoi.send(Nouvelle::Ligne(String::from_utf8_lossy(&courante).into_owned()));
                        courante.clear();
                    }
                } else {
                    courante.push(o);
                }
            }
        }
        if !courante.is_empty() {
            let _ = envoi.send(Nouvelle::Ligne(String::from_utf8_lossy(&courante).into_owned()));
        }
    })
}

/// Lance l'envoi. Toujours en privé : `commande_envoi` n'a pas d'autre sortie.
pub fn envoyer(montee: &Path, video: &Path, d: &Description) -> Result<Envoi, String> {
    let jpg = noyau::chemin_derive(video, noyau::SUFFIXE_JPG);
    let description = std::env::temp_dir().join(format!("monteur2-description-{}.txt", std::process::id()));
    std::fs::write(&description, format!("{}\n", d.texte.trim())).map_err(|e| e.to_string())?;
    let cmd = videos::commande_envoi(montee, d, &description, jpg.exists().then_some(jpg.as_path()));
    let mut enfant = noyau::systeme::commande(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("youtube-envoi : {e}"))?;
    let (envoi, nouvelles) = mpsc::channel();
    let a = suivre_flux(enfant.stdout.take().expect("sortie"), envoi.clone());
    let b = suivre_flux(enfant.stderr.take().expect("erreurs"), envoi.clone());
    std::thread::spawn(move || {
        let reussi = enfant.wait().is_ok_and(|s| s.success());
        let _ = a.join();
        let _ = b.join();
        let _ = envoi.send(Nouvelle::Fin(reussi));
    });
    Ok(Envoi { video: video.to_path_buf(), nouvelles, pourcent: 0.0, sortie: String::new(), description })
}

pub struct Youtube {
    pub description: Description,
    pour: Option<PathBuf>,
    /// Un changement pas encore écrit, et depuis quand.
    a_ecrire: Option<Instant>,
    /// « Envoyer » a été cliqué : on montre ce qui va partir, un second clic envoie.
    confirmer: bool,
    /// L'envoi demandé par le second clic.
    pub demande: bool,
    /// « Titre copié. », trois secondes sous les boutons.
    annonce: Option<(String, Instant)>,
}

/// Les chapitres aux temps de la vidéo envoyée, dans le texte de la description.
pub fn inserer_chapitres(d: &mut Description, video: &Path, montage: &noyau::montage::Montage) {
    let avec_vignette = noyau::chemin_derive(video, noyau::SUFFIXE_PNG).exists();
    let decalage = if avec_vignette { noyau::DUREE_VIGNETTE } else { 0.0 };
    d.texte = noyau::sous_titres::inserer_chapitres(&d.texte, &noyau::sous_titres::texte_chapitres(montage, decalage));
}

impl Youtube {
    pub fn new() -> Self {
        Self { description: Description::default(), pour: None, a_ecrire: None, confirmer: false, demande: false, annonce: None }
    }

    /// Écrit le fichier de description s'il a changé depuis une seconde (ou tout de suite).
    pub fn ecrire_si_besoin(&mut self, maintenant: bool) -> Option<String> {
        let depuis = self.a_ecrire?;
        if !maintenant && depuis.elapsed().as_secs() < 1 {
            return None;
        }
        self.a_ecrire = None;
        let video = self.pour.as_ref()?;
        self.description.ecrire(&noyau::chemin_derive(video, noyau::SUFFIXE_DESCRIPTION)).err().map(|e| format!("Description non écrite : {e}"))
    }

    /// La description est à écrire, même si elle n'a pas changé à la frappe.
    pub fn marquer(&mut self) {
        self.a_ecrire = Some(Instant::now());
    }

    pub fn charger(&mut self, video: &Path) {
        let _ = self.ecrire_si_besoin(true);
        self.description = Description::lire(&noyau::chemin_derive(video, noyau::SUFFIXE_DESCRIPTION)).unwrap_or_default();
        self.pour = Some(video.to_path_buf());
        self.confirmer = false;
    }

    /// La vidéo à envoyer : celle avec vignette, sinon rien.
    pub fn a_envoyer(video: &Path) -> Option<PathBuf> {
        Some(noyau::chemin_derive(video, noyau::SUFFIXE_MONTEE)).filter(|m| m.exists())
    }

    /// `youtube-envoi` est là : l'envoi se fait d'ici. Sinon, par YouTube Studio.
    pub fn envoi_direct() -> bool {
        Path::new(&videos::programme_envoi()).is_file()
    }

    /// Préparer pour YouTube : de quoi remplir YouTube Studio, la vidéo glissée à la main.
    fn preparer_pour_studio(&mut self, ui: &Ui, video: &Path, montee: Option<&Path>) {
        let d = &self.description;
        ui.text_wrapped("Envoi par YouTube Studio : la vidéo montée et sa vignette sont à côté de la prise. Copie les textes un à un, puis glisse la vidéo dans la page de Studio.");
        let mut annonce = None;
        let mut copier = |ui: &Ui, etiquette: &str, fait: &str, texte: &str| {
            if ui.button(etiquette) {
                annonce = Some(if crate::presse_papiers::copier(texte) { format!("{fait} : à coller dans Studio.") } else { "Le presse-papiers ne répond pas.".to_string() });
            }
        };
        copier(ui, "Copier le titre", "Titre copié", &d.titre);
        ui.same_line();
        copier(ui, "Copier la description", "Description copiée", d.texte.trim());
        ui.same_line();
        copier(ui, "Copier les mots-clés", "Mots-clés copiés", &d.mots_cles);
        if let Some(a) = annonce {
            self.annonce = Some((a, Instant::now()));
        }
        if let Some((a, depuis)) = &self.annonce
            && depuis.elapsed().as_secs() < 3
        {
            ui.text_colored(charte::c(charte::SUCCES_TEXTE), a);
        }
        if ui.button_with_size("Ouvrir YouTube Studio", [200.0, 32.0]) {
            let _ = opener::open_browser("https://www.youtube.com/upload");
        }
        ui.same_line();
        if ui.button_with_size("Montrer la vidéo", [160.0, 32.0])
            && let Some(dossier) = montee.unwrap_or(video).parent()
        {
            let _ = opener::open(dossier);
        }
        if let Some(m) = montee {
            ui.text_colored(charte::c(charte::TEXT_2), format!("À glisser : {}", m.file_name().unwrap_or_default().to_string_lossy()));
        }
        let jpg = noyau::chemin_derive(video, noyau::SUFFIXE_JPG);
        if jpg.exists() {
            ui.text_colored(charte::c(charte::TEXT_2), format!("Vignette (Miniature, dans Studio) : {}", jpg.file_name().unwrap_or_default().to_string_lossy()));
        }
    }

    /// Le panneau ; la fiche n'y est que lue.
    pub fn dessiner(&mut self, ui: &Ui, fiche: &Fiche, video: &Path, montage: &noyau::montage::Montage, envoi: Option<&Envoi>) {
        let d = &mut self.description;
        let mut change = false;
        let largeur = ui.push_item_width(-1.0);
        ui.text(format!("Titre  ({} / {})", d.titre.chars().count(), videos::TITRE_MAX));
        change |= ui.input_text("##titre_yt", &mut d.titre).build();
        ui.text(format!("Description  ({} / {} octets)", d.texte.len(), videos::DESCRIPTION_MAX));
        change |= ui.input_text_multiline("##texte_yt", &mut d.texte, [-1.0, 190.0]).build();
        ui.text("Mots-clés, séparés par des virgules");
        change |= ui.input_text("##mots_yt", &mut d.mots_cles).build();
        drop(largeur);
        if ui.button("Proposer d'après la vignette") {
            *d = description_proposee(fiche);
            change = true;
        }
        ui.same_line();
        if ui.button("Insérer les chapitres") {
            inserer_chapitres(d, video, montage);
            change = true;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Aux temps de la vidéo envoyée : coupes retirées, vignette comptée.");
        }
        for p in noyau::sous_titres::problemes_chapitres(montage) {
            ui.text_colored(charte::c(charte::ALERTE), format!("Chapitres : {p}"));
        }
        if change {
            self.a_ecrire = Some(Instant::now());
            self.confirmer = false;
        }
        ui.separator();

        // Avant l'envoi : ce qui manque, en clair.
        let d = &self.description;
        let mut pret = true;
        let mut ligne = |ui: &Ui, bon: bool, texte: &str| {
            ui.text_colored(charte::c(if bon { charte::SUCCES_TEXTE } else { charte::ALERTE }), format!("{} {texte}", if bon { "✓" } else { "!" }));
            pret &= bon;
        };
        let montee = Self::a_envoyer(video);
        ligne(ui, montee.is_some(), "Vidéo montée avec sa vignette (Rendre la vidéo, vignette écrite)");
        let date = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let png = noyau::chemin_derive(video, noyau::SUFFIXE_PNG);
        let vignette_a_jour = montee.as_ref().is_none_or(|m| date(&png) <= date(m));
        ligne(ui, vignette_a_jour, "La vidéo a la dernière vignette");
        for p in videos::problemes_description(d) {
            ligne(ui, false, &p);
        }
        if d.titre.trim().is_empty() {
            pret = false;
        }
        let srt = noyau::chemin_derive(video, noyau::SUFFIXE_SRT);
        if srt.exists() {
            ui.text_colored(charte::c(charte::TEXT_2), "Les sous-titres ne partent pas avec la vidéo : à importer dans Studio.");
        }
        if !fiche.youtube_id.is_empty() {
            ui.text_colored(charte::c(charte::ALERTE), format!("Déjà envoyée : https://youtu.be/{} ({})", fiche.youtube_id, fiche.envoyee_le));
        }
        ui.separator();
        if let Some(e) = envoi {
            ui.text_colored(charte::c(charte::ACCENT_HOVER), format!("Envoi de {} : {:.0} %", e.video.file_stem().unwrap_or_default().to_string_lossy(), e.pourcent));
        } else if !Self::envoi_direct() {
            self.preparer_pour_studio(ui, video, montee.as_deref());
        } else if self.confirmer {
            let taille = montee.as_ref().and_then(|m| std::fs::metadata(m).ok()).map_or(0.0, |m| m.len() as f64 / 1e6);
            ui.text_wrapped(format!("Envoyer « {} » ({taille:.0} Mo) sur ta chaîne, en privé ? La mise en public se fera ensuite dans YouTube Studio.", self.description.titre));
            if !fiche.youtube_id.is_empty() {
                ui.text_colored(charte::c(charte::ERREUR), "Elle a déjà été envoyée : cela fera une deuxième vidéo sur la chaîne.");
            }
            if ui.button_with_size("Oui, envoyer en privé", [200.0, 32.0]) {
                self.demande = true;
                self.confirmer = false;
            }
            ui.same_line();
            if ui.button_with_size("Annuler##envoi", [100.0, 32.0]) {
                self.confirmer = false;
            }
        } else if ui.button_with_size("Envoyer en privé…", [200.0, 32.0]) && pret {
            self.confirmer = true;
        }
    }
}
