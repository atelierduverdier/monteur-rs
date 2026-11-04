//! Inter, la police des vignettes et des étiquettes : mesure et tracé du texte.
//!
//! Le texte est mis en forme par `rustybuzz` (HarfBuzz, comme Qt : crénage, ligatures), puis
//! dessiné comme des chemins, ce que fait aussi QPainter. Les mesures suivent `QFontMetricsF` :
//! la hauteur est l'ascendante plus la descendante de la table `hhea` (sans interligne), au 1/64 de
//! pixel, comme FreeType les rend à Qt.

use rustybuzz::ttf_parser::{GlyphId, OutlineBuilder};
use rustybuzz::{Face, UnicodeBuffer};
use tiny_skia::{Path, PathBuilder};

const INTER: &[u8] = include_bytes!("../../../ressources/polices/Inter-Regular.ttf");
const INTER_GRAS: &[u8] = include_bytes!("../../../ressources/polices/Inter-SemiBold.ttf");

pub struct Police {
    face: Face<'static>,
    upem: f32,
}

/// `floor` au 1/64 de pixel (les métriques de FreeType).
fn soixante_quatrieme(x: f32) -> f32 {
    (x * 64.0).floor() / 64.0
}

impl Police {
    fn nouvelle(octets: &'static [u8]) -> Self {
        let face = Face::from_slice(octets, 0).expect("Inter se lit");
        let upem = face.units_per_em() as f32;
        Self { face, upem }
    }

    pub fn inter() -> Self {
        Self::nouvelle(INTER)
    }

    pub fn inter_gras() -> Self {
        Self::nouvelle(INTER_GRAS)
    }

    pub fn ascent(&self, px: f32) -> f32 {
        soixante_quatrieme(self.face.ascender() as f32 * px / self.upem)
    }

    pub fn descent(&self, px: f32) -> f32 {
        soixante_quatrieme(-(self.face.descender() as f32) * px / self.upem)
    }

    /// `QFontMetricsF.height()` : ascendante plus descendante.
    pub fn hauteur(&self, px: f32) -> f32 {
        self.ascent(px) + self.descent(px)
    }

    /// Les glyphes du texte et leur position (x depuis le départ, en pixels).
    fn former(&self, texte: &str, px: f32) -> (Vec<(GlyphId, f32, f32)>, f32) {
        let mut tampon = UnicodeBuffer::new();
        tampon.push_str(texte);
        let forme = rustybuzz::shape(&self.face, &[], tampon);
        let k = px / self.upem;
        let mut x = 0.0;
        let mut glyphes = Vec::with_capacity(forme.len());
        for (info, pos) in forme.glyph_infos().iter().zip(forme.glyph_positions()) {
            glyphes.push((GlyphId(info.glyph_id as u16), x + pos.x_offset as f32 * k, pos.y_offset as f32 * k));
            x += pos.x_advance as f32 * k;
        }
        (glyphes, x)
    }

    /// La position de chaque glyphe sur la ligne (x depuis le départ, en pixels).
    pub fn positions(&self, texte: &str, px: f32) -> Vec<f32> {
        self.former(texte, px).0.into_iter().map(|g| g.1).collect()
    }

    /// `QFontMetricsF.horizontalAdvance()`.
    pub fn avance(&self, texte: &str, px: f32) -> f32 {
        self.former(texte, px).1
    }

    /// Le contour du texte, la ligne de base passant par (x, y).
    pub fn chemin(&self, texte: &str, px: f32, x: f32, y: f32) -> Option<Path> {
        let (glyphes, _) = self.former(texte, px);
        let mut trace = Trace { pb: PathBuilder::new(), k: px / self.upem, x: 0.0, y };
        for (id, gx, gy) in glyphes {
            trace.x = x + gx;
            trace.y = y - gy;
            self.face.outline_glyph(id, &mut trace);
        }
        trace.pb.finish()
    }
}

struct Trace {
    pb: PathBuilder,
    k: f32,
    x: f32,
    y: f32,
}

impl Trace {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.k, self.y - y * self.k)
    }
}

impl OutlineBuilder for Trace {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.pb.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.p(x, y);
        self.pb.line_to(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ((x1, y1), (x, y)) = (self.p(x1, y1), self.p(x, y));
        self.pb.quad_to(x1, y1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ((x1, y1), (x2, y2), (x, y)) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.pb.cubic_to(x1, y1, x2, y2, x, y);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// Les deux graisses de la charte.
pub struct Polices {
    pub normale: Police,
    pub grasse: Police,
}

impl Polices {
    pub fn charger() -> Self {
        Self { normale: Police::inter(), grasse: Police::inter_gras() }
    }

    pub fn de(&self, gras: bool) -> &Police {
        if gras { &self.grasse } else { &self.normale }
    }
}

/// `police(pixels)` du Monteur 1 : la taille est arrondie au pixel entier, 6 au moins.
pub fn taille(pixels: f64) -> f32 {
    (pixels.round_ties_even() as i64).max(6) as f32
}
