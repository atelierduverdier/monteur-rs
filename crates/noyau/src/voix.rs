//! Le nettoyage de la voix : mêmes réglages pour toutes les prises, gardés à côté des fiches. La
//! chaîne de filtres ffmpeg sert au rendu comme à l'écoute avant/après.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Voix {
    pub bruit: bool,
    /// 0 à 1.
    pub bruit_force: f64,
    pub grave: bool,
    pub grave_hz: f64,
    pub egaliser: bool,
    pub rapport: f64,
    pub seuil_db: f64,
    pub adoucir: bool,
    /// 0 à 1.
    pub adoucir_force: f64,
    pub niveau: bool,
    pub niveau_lufs: f64,
    pub crete_db: f64,
}

impl Default for Voix {
    fn default() -> Self {
        Self {
            bruit: true,
            bruit_force: 0.5,
            grave: true,
            grave_hz: 80.0,
            egaliser: true,
            rapport: 3.0,
            seuil_db: -20.0,
            adoucir: false,
            adoucir_force: 0.3,
            niveau: true,
            niveau_lufs: -14.0,
            crete_db: -1.0,
        }
    }
}

impl Voix {
    /// Rien d'appliqué : le son d'origine.
    pub fn aucune() -> Self {
        Self { bruit: false, grave: false, egaliser: false, adoucir: false, niveau: false, ..Self::default() }
    }

    /// Un fichier absent ou abîmé donne les réglages par défaut.
    pub fn lire(chemin: &Path) -> Self {
        std::fs::read_to_string(chemin).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn ecrire(&self, chemin: &Path) -> std::io::Result<()> {
        let texte = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::ecriture::ecrire_atomique(chemin, "voix", texte.as_bytes())
    }

    /// Les filtres avant le réglage du niveau : le bruit d'abord, pour que le compresseur ne le
    /// fasse pas remonter. `None` : rien à faire.
    pub fn filtres(&self) -> Option<String> {
        let mut f = Vec::new();
        if self.grave {
            f.push(format!("highpass=f={:.0}", self.grave_hz));
        }
        if self.bruit {
            // nf : la voix n'est pas prise pour du bruit au-dessus de −50 dB.
            f.push(format!("afftdn=nr={:.1}:nf=-50:tn=1", 2.0 + self.bruit_force * 28.0));
        }
        if self.adoucir {
            f.push(format!("deesser=i={:.2}", self.adoucir_force));
        }
        if self.egaliser {
            f.push(format!("acompressor=threshold={:.1}dB:ratio={:.1}:attack=5:release=150", self.seuil_db, self.rapport));
        }
        (!f.is_empty()).then(|| f.join(","))
    }

    /// La cible de `loudnorm`.
    pub fn cible(&self) -> String {
        format!("I={:.1}:TP={:.1}:LRA=11", self.niveau_lufs, self.crete_db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaine_dans_l_ordre() {
        assert_eq!(Voix::aucune().filtres(), None);
        let f = Voix { adoucir: true, ..Voix::default() }.filtres().unwrap();
        let rang = |m: &str| f.find(m).unwrap();
        assert!(rang("highpass") < rang("afftdn") && rang("afftdn") < rang("deesser") && rang("deesser") < rang("acompressor"), "{f}");
        assert_eq!(Voix::default().cible(), "I=-14.0:TP=-1.0:LRA=11");
    }

    #[test]
    fn fichier_abime_ou_absent() {
        let d = std::env::temp_dir().join(format!("voix-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let c = d.join("voix.json");
        assert_eq!(Voix::lire(&c), Voix::default());
        let v = Voix { rapport: 5.0, adoucir: true, ..Voix::default() };
        v.ecrire(&c).unwrap();
        assert_eq!(Voix::lire(&c), v);
        std::fs::write(&c, "pas du json").unwrap();
        assert_eq!(Voix::lire(&c), Voix::default());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
