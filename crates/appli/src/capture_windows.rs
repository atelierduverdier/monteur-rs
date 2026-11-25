//! Filmer une fenêtre sous Windows. Windows.Graphics.Capture (crate `windows-capture`) donne ses images, même quand
//! une autre fenêtre passe devant, avec le pointeur, sans le cadre jaune (Windows 11). Chaque image est recadrée sur
//! la zone cliente : sans barre de titre ni bords, comme sous KDE, et c'est là que VerdierCAM note ses gestes.
//! `enregistreur.rs` fait le reste : le rythme, l'encodage, le son du bureau et le micro, la prise rangée.
//!
//! La fenêtre se choisit dans le Monteur (`ChoixFenetre`), à chaque fois, la dernière filmée déjà choisie
//! (`fenetre-capture`, à côté des fiches) ; Filmer VerdierCAM prend celle de VerdierCAM dès qu'elle s'ouvre.
//!
//! TOUT L'ÉCRAN (25/11/2026, Christophe : « parfois, on veut enregistrer plusieurs fenêtres […] il faudrait le bureau
//! complet ») : chaque écran est en tête de la liste, avant les fenêtres. Sous KDE, le dialogue du portail le
//! proposait déjà ; ici, seules les fenêtres l'étaient. Un écran se filme entier, sans recadrage.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Graphics::Capture::{GraphicsCaptureAccess, GraphicsCaptureAccessKind};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::monitor::Monitor;
use windows_capture::window::Window;

use crate::enregistreur::{Demarrer, Dernier, Image, Sons, Source};
pub use crate::enregistreur::{Enregistrement as Capture, Fin};

/// Une fenêtre qu'on peut filmer, ou un écran entier.
#[derive(Clone, Debug, PartialEq)]
pub struct Fenetre {
    /// 0 pour un écran.
    pub hwnd: isize,
    pub titre: String,
    /// Le programme, `VerdierCAM.exe` par exemple ; la taille pour un écran.
    pub programme: String,
    /// Un écran entier : son rang pour Windows (à partir de 1).
    pub ecran: Option<usize>,
}

/// Le titre d'un écran dans la liste, et dans `fenetre-capture` quand c'est lui qui a été filmé.
const TOUT_L_ECRAN: &str = "Tout l'écran";

/// Les écrans, « Tout l'écran » s'il n'y en a qu'un, numérotés sinon.
fn ecrans() -> Vec<Fenetre> {
    let tous = Monitor::enumerate().unwrap_or_default();
    let seul = tous.len() == 1;
    tous.iter()
        .enumerate()
        .map(|(i, m)| Fenetre {
            hwnd: 0,
            titre: if seul { TOUT_L_ECRAN.to_string() } else { format!("{TOUT_L_ECRAN} {}", i + 1) },
            programme: match (m.width(), m.height()) {
                (Ok(l), Ok(h)) => format!("{l} × {h}"),
                _ => String::new(),
            },
            ecran: Some(i + 1),
        })
        .collect()
}

/// Le processus d'une fenêtre : le Monteur reconnaît ainsi le VerdierCAM qu'il a lancé.
pub fn processus(hwnd: isize) -> Option<u32> {
    Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void).process_id().ok()
}

/// Les fenêtres visibles et titrées, le Monteur excepté.
pub fn fenetres() -> Vec<Fenetre> {
    let moi = std::process::id();
    Window::enumerate()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.process_id().is_ok_and(|p| p != moi))
        .filter_map(|w| {
            let titre = w.title().ok().filter(|t| !t.trim().is_empty())?;
            Some(Fenetre { hwnd: w.as_raw_hwnd() as isize, titre, programme: w.process_name().unwrap_or_default(), ecran: None })
        })
        .collect()
}

/// La première fenêtre de ce programme (`VerdierCAM.exe`), s'il en a une d'ouverte.
pub fn fenetre_du_programme(programme: &str) -> Option<Fenetre> {
    fenetres().into_iter().find(|f| f.programme.eq_ignore_ascii_case(programme))
}

/// La dernière fenêtre filmée, si elle est encore ouverte : la même, ou à défaut une du même programme (le titre de
/// VerdierCAM change avec le projet ouvert).
pub fn fenetre_gardee(chemin: &Path) -> Option<Fenetre> {
    let texte = std::fs::read_to_string(chemin).ok()?;
    let mut lignes = texte.lines();
    let (programme, titre) = (lignes.next()?.trim().to_string(), lignes.next().unwrap_or("").trim().to_string());
    if titre.starts_with(TOUT_L_ECRAN) {
        let ecrans = ecrans();
        return ecrans.iter().find(|e| e.titre == titre).or(ecrans.first()).cloned();
    }
    let ouvertes: Vec<Fenetre> = fenetres().into_iter().filter(|f| f.programme.eq_ignore_ascii_case(&programme)).collect();
    ouvertes.iter().find(|f| f.titre == titre).or(ouvertes.first()).cloned()
}

pub fn garder(chemin: &Path, f: &Fenetre) {
    if let Some(d) = chemin.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(chemin, format!("{}\n{}\n", f.programme, f.titre));
}

/// Filmer `fenetre` : le bureau et le micro, chacun sur sa piste, mêlés à la fin.
pub fn lancer(dossier: &Path, fenetre: &Fenetre, avec_decompte: bool) -> Result<Capture, String> {
    demander_sans_cadre();
    Capture::lancer(dossier, demarrer(fenetre.hwnd, fenetre.ecran), Sons { bureau: true, micro: true }, avec_decompte)
}

/// LA CAPTURE SANS CADRE JAUNE (23/11/2026, Christophe : « Ajoute la demande de capture sans cadre jaune »). Un
/// programme empaqueté (le paquet du Store, outils/construire-msix.py) ne filme sans cadre qu'après l'accord de
/// l'utilisateur : `GraphicsCaptureAccess.RequestAccessAsync(Borderless)`, que son manifeste permet
/// (`graphicsCaptureWithoutBorder`). Sinon Windows accepte « sans cadre » sans erreur, mais garde le cadre.
/// Windows pose la question une fois et retient la réponse ; les fois suivantes, elle revient tout de suite.
///
/// La demande part dès qu'on clique Filmer (ou Filmer VerdierCAM), dans un fil à elle : la fenêtre du Monteur ne se
/// fige pas pendant que Windows pose sa question, et le décompte laisse le temps d'y répondre. `demarrer` attend la
/// réponse une demi-seconde au plus (le cas d'une réponse déjà donnée), puis filme : sans réponse, avec le cadre.
/// Hors paquet (le zip), ou sur un Windows 10 d'avant cette demande (build 20348), elle échoue ou revient accordée :
/// rien ne change.
static SANS_CADRE: Mutex<Option<Receiver<()>>> = Mutex::new(None);

pub fn demander_sans_cadre() {
    let Ok(mut attente) = SANS_CADRE.lock() else { return };
    if attente.is_some() {
        return;
    }
    let (fini, recu) = std::sync::mpsc::channel();
    *attente = Some(recu);
    let _ = std::thread::Builder::new().name("capture-sans-cadre".into()).spawn(move || {
        let _ = GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless).and_then(|demande| demande.join());
        let _ = fini.send(());
    });
}

/// Laisse à la demande, si elle court encore, une demi-seconde pour aboutir.
fn attendre_sans_cadre() {
    if let Ok(attente) = SANS_CADRE.lock()
        && let Some(recu) = attente.as_ref()
    {
        let _ = recu.recv_timeout(Duration::from_millis(500));
    }
}

/// La zone cliente de la fenêtre dans l'image filmée, qui couvre son cadre visible (DWM).
fn zone(hwnd: isize, l: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let hwnd = HWND(hwnd as *mut std::ffi::c_void);
    let mut cadre = RECT::default();
    let mut client = RECT::default();
    let mut origine = POINT { x: 0, y: 0 };
    // SAFETY : des appels Win32 sur une fenêtre qui peut avoir disparu ; ils échouent alors proprement.
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut cadre).cast(), std::mem::size_of::<RECT>() as u32).ok()?;
        GetClientRect(hwnd, &mut client).ok()?;
        if !ClientToScreen(hwnd, &mut origine).as_bool() {
            return None;
        }
    }
    crate::enregistreur::zone_cliente((cadre.left, cadre.top), (origine.x, origine.y), (client.right - client.left, client.bottom - client.top), (l, h))
}

/// Reçoit les images de Windows et pose la dernière dans la boîte.
struct Prendre {
    dernier: Dernier,
    hwnd: isize,
    tampon: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Prendre {
    type Flags = (Dernier, isize);
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (dernier, hwnd) = ctx.flags;
        Ok(Self { dernier, hwnd, tampon: Vec::new() })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _controle: InternalCaptureControl) -> Result<(), Self::Error> {
        let (l, h) = (frame.width(), frame.height());
        // Un écran (hwnd 0) se filme entier.
        let (x0, y0, x1, y1) = if self.hwnd == 0 { (0, 0, l, h) } else { zone(self.hwnd, l, h).unwrap_or((0, 0, l, h)) };
        // Une image ratée (fenêtre en train de se réduire…) se saute : la précédente reste.
        let Ok(tampon_image) = frame.buffer_crop(x0, y0, x1, y1) else { return Ok(()) };
        let (largeur, hauteur) = (tampon_image.width(), tampon_image.height());
        let bgra = tampon_image.as_nopadding_buffer(&mut self.tampon).to_vec();
        if let Ok(mut b) = self.dernier.lock() {
            b.image = Some(Arc::new(Image { largeur, hauteur, bgra }));
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        if let Ok(mut b) = self.dernier.lock() {
            b.fermee = true;
        }
        Ok(())
    }
}

struct EnMarche(Option<CaptureControl<Prendre, String>>);

impl Source for EnMarche {
    fn arreter(&mut self) {
        if let Some(c) = self.0.take() {
            let _ = c.stop();
        }
    }
}

impl Drop for EnMarche {
    fn drop(&mut self) {
        self.arreter();
    }
}

/// Filmer une fenêtre ou un écran : le même Prendre, seule la chose filmée change.
fn filmer<T>(chose: impl Fn() -> T, dernier: &Dernier, hwnd: isize) -> Result<CaptureControl<Prendre, String>, String>
where
    T: TryInto<windows_capture::settings::GraphicsCaptureItemType> + Send + 'static,
    <T as TryInto<windows_capture::settings::GraphicsCaptureItemType>>::Error: Send + Sync + std::fmt::Debug + 'static,
{
    let reglages = |bord| {
        Settings::new(
            chose(),
            CursorCaptureSettings::WithCursor,
            bord,
            SecondaryWindowSettings::Default,
            // 30 images par seconde suffisent : au-delà, des copies pour rien.
            MinimumUpdateIntervalSettings::Custom(Duration::from_millis(30)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (dernier.clone(), hwnd),
        )
    };
    // Sans le cadre jaune si Windows le permet (Windows 11, et l'accord donné pour le paquet du Store), avec sinon.
    attendre_sans_cadre();
    Prendre::start_free_threaded(reglages(DrawBorderSettings::WithoutBorder))
        .or_else(|_| Prendre::start_free_threaded(reglages(DrawBorderSettings::Default)))
        .map_err(|e| format!("la capture de Windows refuse cette fenêtre ({e:?})"))
}

fn demarrer(hwnd: isize, ecran: Option<usize>) -> Demarrer {
    Box::new(move |dernier: Dernier| {
        let controle = match ecran {
            Some(rang) => {
                let m = Monitor::from_index(rang).or_else(|_| Monitor::primary()).map_err(|e| format!("pas d'écran à filmer ({e:?})"))?;
                filmer(|| m, &dernier, 0)?
            }
            None => filmer(|| Window::from_raw_hwnd(hwnd as *mut std::ffi::c_void), &dernier, hwnd)?,
        };
        Ok(Box::new(EnMarche(Some(controle))) as Box<dyn Source>)
    })
}

/// Le choix de la fenêtre à filmer, dans une boîte du Monteur.
pub struct ChoixFenetre {
    pub fenetres: Vec<Fenetre>,
    /// Enregistrement simple : le choix n'est pas gardé.
    pub simple: bool,
    pub choisie: Option<usize>,
    /// La boîte est ouverte (ImGui l'ouvre une fois).
    pub ouverte: bool,
}

impl ChoixFenetre {
    pub fn nouveau(simple: bool, gardee: Option<&Path>) -> Self {
        let fenetres = ecrans().into_iter().chain(fenetres()).collect::<Vec<_>>();
        let choisie = gardee.and_then(fenetre_gardee).and_then(|g| fenetres.iter().position(|f| f.hwnd == g.hwnd && f.ecran == g.ecran));
        Self { fenetres, simple, choisie, ouverte: false }
    }

    pub fn rafraichir(&mut self) {
        let avant = self.choisie.and_then(|i| self.fenetres.get(i).map(|f| (f.hwnd, f.ecran)));
        self.fenetres = ecrans().into_iter().chain(fenetres()).collect();
        self.choisie = avant.and_then(|h| self.fenetres.iter().position(|f| (f.hwnd, f.ecran) == h));
    }
}

