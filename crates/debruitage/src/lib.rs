//! Le débruitage de la voix par DeepFilterNet 3 : un petit réseau qui sépare la voix du bruit d'atelier
//! (ventilation, ronflement, souffle), là où le filtre `afftdn` de ffmpeg ne changeait presque rien. Le
//! modèle (8 Mo) est inclus dans le programme : rien à télécharger, sous Linux comme sous Windows.
//!
//! Il travaille en 48 kHz mono, sur le processeur, par blocs de 10 ms.

use df::tract::{DfParams, DfTract, RuntimeParams};
use ndarray::{ArrayView2, ArrayViewMut2};

/// La fréquence d'échantillonnage du modèle.
pub const TAUX: u32 = 48_000;

/// La force du réglage (0 à 1) en atténuation maximale du bruit, en dB. À fond, aucune limite : le
/// bruit disparaît tout à fait, mais la voix peut sonner « sous l'eau » ; plus bas, un peu du son
/// d'origine reste mêlé et la voix garde son timbre.
pub fn attenuation_db(force: f64) -> f32 {
    let f = force.clamp(0.0, 1.0);
    if f >= 0.995 { 100.0 } else { (6.0 + f * 54.0) as f32 }
}

fn erreur(e: impl std::fmt::Display) -> String {
    format!("DeepFilterNet : {e}")
}

/// Le modèle en marche sur un son qui arrive par morceaux (une prise d'une heure ne tient pas en
/// mémoire). Le retard du modèle (sa fenêtre et son regard en avant, 30 ms) est rattrapé : ce qui
/// sort est calé sur ce qui entre, et aussi long.
pub struct Debruiteur {
    modele: DfTract,
    bloc: usize,
    /// Ce qui sort d'abord n'est que le retard : à jeter.
    a_jeter: usize,
    attente: Vec<f32>,
    entres: usize,
    sortis: usize,
}

impl Debruiteur {
    /// `attenuation_db` : voir [`attenuation_db`] (100 : sans limite).
    pub fn new(attenuation_db: f32) -> Result<Self, String> {
        // Les seuils de l'outil deep-filter, qui rendent ce que rend DeepFilterNet en Python. Ceux de
        // `default_with_ch` (−10, 30, 20) sautent des étages du modèle sur la voix : elle sortait 5 dB plus
        // bas, ressemblant à 0,83 à la version validée à l'écoute (Christophe, 26/11/2026), contre 0,998 ici.
        let parametres = RuntimeParams::default_with_ch(1).with_atten_lim(attenuation_db).with_thresholds(-15.0, 35.0, 35.0);
        let modele = DfTract::new(DfParams::default(), &parametres).map_err(erreur)?;
        let bloc = modele.hop_size;
        let a_jeter = modele.fft_size - modele.hop_size + modele.lookahead * modele.hop_size;
        Ok(Self { modele, bloc, a_jeter, attente: Vec::new(), entres: 0, sortis: 0 })
    }

    fn traiter(&mut self, sortie: &mut Vec<f32>) -> Result<(), String> {
        let blocs = self.attente.len() / self.bloc;
        let mut net = vec![0f32; self.bloc];
        for e in self.attente.chunks_exact(self.bloc) {
            let e = ArrayView2::from_shape((1, self.bloc), e).map_err(erreur)?;
            let s = ArrayViewMut2::from_shape((1, self.bloc), &mut net[..]).map_err(erreur)?;
            self.modele.process(e, s).map_err(erreur)?;
            let jete = self.a_jeter.min(self.bloc);
            self.a_jeter -= jete;
            let garde = (self.bloc - jete).min(self.entres - self.sortis);
            sortie.extend_from_slice(&net[jete..jete + garde]);
            self.sortis += garde;
        }
        self.attente.drain(..blocs * self.bloc);
        Ok(())
    }

    /// Un morceau de son mono à 48 kHz ; ce qui est prêt s'ajoute à `sortie`.
    pub fn pousser(&mut self, son: &[f32], sortie: &mut Vec<f32>) -> Result<(), String> {
        self.attente.extend_from_slice(son);
        self.entres += son.len();
        self.traiter(sortie)
    }

    /// La fin du son : du silence pousse dehors ce que le modèle retenait encore.
    pub fn finir(mut self, sortie: &mut Vec<f32>) -> Result<(), String> {
        let n = (self.attente.len() + self.a_jeter + self.bloc * 3).div_ceil(self.bloc) * self.bloc;
        self.attente.resize(n, 0.0);
        self.traiter(sortie)
    }
}

/// Débruite d'un coup un son mono à 48 kHz.
pub fn debruiter(son: &[f32], attenuation_db: f32) -> Result<Vec<f32>, String> {
    let mut d = Debruiteur::new(attenuation_db)?;
    let mut sortie = Vec::with_capacity(son.len());
    d.pousser(son, &mut sortie)?;
    d.finir(&mut sortie)?;
    Ok(sortie)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Par petits morceaux ou d'un coup : le même son, de la même longueur.
    #[test]
    fn par_morceaux_comme_d_un_coup() {
        let son: Vec<f32> = (0..20_000).map(|i| (i as f32 * 0.07).sin() * 0.3 + ((i * 7919) % 101) as f32 / 1000.0).collect();
        let entier = debruiter(&son, 30.0).unwrap();
        let mut d = Debruiteur::new(30.0).unwrap();
        let mut morceaux = Vec::new();
        for m in son.chunks(333) {
            d.pousser(m, &mut morceaux).unwrap();
        }
        d.finir(&mut morceaux).unwrap();
        assert_eq!(entier.len(), son.len());
        assert_eq!(morceaux, entier);
        assert_eq!(debruiter(&son[..100], 30.0).unwrap().len(), 100);
    }

    #[test]
    fn la_force_en_decibels() {
        assert_eq!(attenuation_db(0.0), 6.0);
        assert_eq!(attenuation_db(0.5), 33.0);
        assert_eq!(attenuation_db(1.0), 100.0);
        assert_eq!(attenuation_db(-1.0), 6.0);
    }

    /// Un souffle continu, une « voix » (des harmoniques de 150 Hz) au milieu : après, le souffle
    /// seul a beaucoup baissé, la voix est toujours là et rien n'a glissé dans le temps.
    #[test]
    fn le_souffle_baisse_la_voix_reste() {
        let mut graine = 1u32;
        let mut bruit = || {
            graine = graine.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (graine >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        let n = 3 * TAUX as usize;
        let voix = |i: usize| {
            let t = i as f32 / TAUX as f32;
            // Une hauteur qui glisse, de 120 à 220 Hz : sans période, un décalage se verrait.
            let phase = std::f32::consts::TAU * (120.0 * t + 50.0 * (t - 1.0) * (t - 1.0));
            if (1.0..2.0).contains(&t) { (1..12).map(|k| (phase * k as f32).sin() / k as f32).sum::<f32>() * 0.2 } else { 0.0 }
        };
        let voix: Vec<f32> = (0..n + 2000).map(voix).collect();
        let son: Vec<f32> = (0..n).map(|i| voix[i] + bruit() * 0.02).collect();
        let net = debruiter(&son, 100.0).unwrap();
        assert_eq!(net.len(), son.len());
        let efficace = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
        let (souffle_avant, souffle_apres) = (efficace(&son[..TAUX as usize / 2]), efficace(&net[TAUX as usize / 4..TAUX as usize * 3 / 4]));
        assert!(souffle_apres < souffle_avant / 10.0, "souffle {souffle_avant} → {souffle_apres}");
        let milieu = TAUX as usize * 5 / 4..TAUX as usize * 7 / 4;
        let (voix_avant, voix_apres) = (efficace(&son[milieu.clone()]), efficace(&net[milieu.clone()]));
        // Un bourdonnement de synthèse n'est pas une vraie voix : le modèle le baisse un peu.
        assert!(voix_apres > voix_avant / 4.0, "voix {voix_avant} → {voix_apres}");
        // Calée : la voix débruitée ressemble le plus à la voix d'origine au même instant.
        let ressemblance = |d: i64| milieu.clone().map(|i| net[i] * voix[(i as i64 + d) as usize]).sum::<f32>();
        let meilleur = (-2000..=2000).step_by(4).max_by(|a, b| ressemblance(*a).total_cmp(&ressemblance(*b))).unwrap();
        assert!(meilleur.abs() <= 8, "décalée de {meilleur} échantillons");
    }
}
