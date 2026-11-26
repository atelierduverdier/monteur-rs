//! Choisir plusieurs prises dans la liste, comme dans un gestionnaire de fichiers, pour les mettre à
//! la corbeille d'un coup (Christophe, 26/11/2026 : « pouvoir sélectionner plusieurs projets pour les
//! effacer tous d'un coup »). Un clic ouvre la prise ; Ctrl+clic en ajoute ou en retire une ; Maj+clic
//! prend tout l'intervalle depuis la dernière cliquée. Sans choix multiple, c'est la prise ouverte qui
//! est en surbrillance, comme avant.

use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
pub struct Selection {
    /// Les prises choisies ; vide : pas de choix multiple, seule l'ouverte compte.
    choisies: Vec<PathBuf>,
    /// D'où part un Maj+clic : la dernière prise cliquée.
    ancre: Option<PathBuf>,
}

impl Selection {
    /// Un clic sur la ligne `i` de `liste` ; `true` : il faut ouvrir cette prise (clic simple).
    pub fn cliquer(&mut self, liste: &[PathBuf], i: usize, ctrl: bool, maj: bool, ouverte: Option<&Path>) -> bool {
        let Some(chemin) = liste.get(i) else { return false };
        if maj {
            // Depuis la dernière cliquée, sinon l'ouverte, sinon celle-ci.
            let depart = self
                .ancre
                .as_deref()
                .or(ouverte)
                .and_then(|a| liste.iter().position(|p| p == a))
                .unwrap_or(i);
            let (a, b) = (depart.min(i), depart.max(i));
            if !ctrl {
                self.choisies.clear();
            }
            for p in &liste[a..=b] {
                if !self.choisies.contains(p) {
                    self.choisies.push(p.clone());
                }
            }
            if self.ancre.is_none() {
                self.ancre = Some(liste[depart].clone());
            }
            return false;
        }
        self.ancre = Some(chemin.clone());
        if ctrl {
            // Le premier Ctrl+clic part de la prise ouverte : elle reste choisie, comme à l'écran.
            if self.choisies.is_empty()
                && let Some(o) = ouverte.filter(|o| liste.iter().any(|p| p == o))
            {
                self.choisies.push(o.to_path_buf());
            }
            match self.choisies.iter().position(|p| p == chemin) {
                Some(k) => {
                    self.choisies.remove(k);
                }
                None => self.choisies.push(chemin.clone()),
            }
            return false;
        }
        self.choisies.clear();
        true
    }

    /// En surbrillance dans la liste ?
    pub fn est_choisie(&self, chemin: &Path, ouverte: Option<&Path>) -> bool {
        if self.choisies.is_empty() { ouverte == Some(chemin) } else { self.choisies.iter().any(|p| p == chemin) }
    }

    /// Plusieurs prises choisies : la liste est en choix multiple.
    pub fn nombre(&self) -> usize {
        self.choisies.len()
    }

    /// Ce que met à la corbeille un clic droit sur `chemin` : toutes les choisies s'il en fait partie
    /// et qu'il y en a plusieurs, sinon lui seul, comme avant. Dans l'ordre de la liste.
    pub fn a_jeter(&self, liste: &[PathBuf], chemin: &Path) -> Vec<PathBuf> {
        if self.choisies.len() > 1 && self.choisies.iter().any(|p| p == chemin) {
            self.choisies_dans(liste)
        } else {
            vec![chemin.to_path_buf()]
        }
    }

    /// Les choisies, dans l'ordre de la liste.
    pub fn choisies_dans(&self, liste: &[PathBuf]) -> Vec<PathBuf> {
        liste.iter().filter(|p| self.choisies.contains(p)).cloned().collect()
    }

    /// Toutes les prises de la liste.
    pub fn tout(&mut self, liste: &[PathBuf]) {
        self.choisies = liste.to_vec();
    }

    pub fn vider(&mut self) {
        self.choisies.clear();
        self.ancre = None;
    }

    /// La liste a été relue : ce qui n'y est plus (à la corbeille, rangé ailleurs) n'est plus choisi.
    pub fn garder(&mut self, liste: &[PathBuf]) {
        self.choisies.retain(|p| liste.contains(p));
        if self.ancre.as_ref().is_some_and(|a| !liste.contains(a)) {
            self.ancre = None;
        }
        // Une seule restante n'est plus un choix multiple : la surbrillance revient à l'ouverte.
        if self.choisies.len() < 2 {
            self.choisies.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn liste() -> Vec<PathBuf> {
        (0..6).map(|i| PathBuf::from(format!("/p/{i}.mp4"))).collect()
    }

    #[test]
    fn clic_simple_ouvre_et_ne_garde_rien() {
        let l = liste();
        let mut s = Selection::default();
        assert!(s.cliquer(&l, 2, false, false, None));
        assert_eq!(s.nombre(), 0);
        assert!(s.est_choisie(&l[2], Some(&l[2])));
        assert!(!s.est_choisie(&l[1], Some(&l[2])));
        // Un clic droit sans choix multiple : la prise cliquée seule, comme avant.
        assert_eq!(s.a_jeter(&l, &l[4]), vec![l[4].clone()]);
    }

    #[test]
    fn ctrl_clic_ajoute_a_l_ouverte_puis_retire() {
        let l = liste();
        let mut s = Selection::default();
        assert!(s.cliquer(&l, 1, false, false, None));
        assert!(!s.cliquer(&l, 3, true, false, Some(&l[1])));
        assert!(!s.cliquer(&l, 4, true, false, Some(&l[1])));
        assert_eq!(s.choisies_dans(&l), vec![l[1].clone(), l[3].clone(), l[4].clone()]);
        // Un second Ctrl+clic retire, même l'ouverte.
        s.cliquer(&l, 1, true, false, Some(&l[1]));
        assert!(!s.est_choisie(&l[1], Some(&l[1])));
        assert_eq!(s.a_jeter(&l, &l[4]), vec![l[3].clone(), l[4].clone()]);
        // Clic droit hors du choix : celle-là seule.
        assert_eq!(s.a_jeter(&l, &l[0]), vec![l[0].clone()]);
    }

    #[test]
    fn maj_clic_prend_l_intervalle_dans_les_deux_sens() {
        let l = liste();
        let mut s = Selection::default();
        s.cliquer(&l, 4, false, false, None);
        s.cliquer(&l, 1, false, true, Some(&l[4]));
        assert_eq!(s.choisies_dans(&l), l[1..=4].to_vec());
        // Un autre Maj+clic repart de la même ancre et remplace l'intervalle.
        s.cliquer(&l, 5, false, true, Some(&l[4]));
        assert_eq!(s.choisies_dans(&l), l[4..=5].to_vec());
        // Ctrl+Maj ajoute l'intervalle à ce qui est choisi.
        s.cliquer(&l, 0, true, false, Some(&l[4]));
        s.cliquer(&l, 1, true, true, Some(&l[4]));
        assert_eq!(s.choisies_dans(&l), vec![l[0].clone(), l[1].clone(), l[4].clone(), l[5].clone()]);
    }

    #[test]
    fn maj_clic_sans_ancre_part_de_l_ouverte() {
        let l = liste();
        let mut s = Selection::default();
        s.cliquer(&l, 3, false, true, Some(&l[0]));
        assert_eq!(s.choisies_dans(&l), l[0..=3].to_vec());
        // Sans ouverte non plus : celle-ci seule.
        let mut s = Selection::default();
        s.cliquer(&l, 2, false, true, None);
        assert_eq!(s.choisies_dans(&l), vec![l[2].clone()]);
    }

    #[test]
    fn la_liste_relue_oublie_les_disparues() {
        let l = liste();
        let mut s = Selection::default();
        s.tout(&l);
        let reste: Vec<_> = l[..3].to_vec();
        s.garder(&reste);
        assert_eq!(s.nombre(), 3);
        s.garder(&l[..1]);
        assert_eq!(s.nombre(), 0);
        // Après la corbeille, un clic simple revient au comportement d'avant.
        assert!(s.cliquer(&l, 0, false, false, None));
    }

    #[test]
    fn clic_hors_liste_ne_fait_rien() {
        let l = liste();
        let mut s = Selection::default();
        assert!(!s.cliquer(&l, 9, false, false, None));
        assert_eq!(s.nombre(), 0);
    }
}
