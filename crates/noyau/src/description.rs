//! La description YouTube d'une vidéo (`<vidéo> description YouTube.txt`) : TITRE, DESCRIPTION,
//! MOTS-CLÉS, comme le Monteur 1 et les sessions d'avant lui l'écrivaient.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Description {
    pub titre: String,
    pub texte: String,
    pub mots_cles: String,
}

impl Description {
    pub fn vide(&self) -> bool {
        self.titre.trim().is_empty() && self.texte.trim().is_empty() && self.mots_cles.trim().is_empty()
    }

    /// Ce qui précède le premier en-tête (ou tout le fichier, s'il n'en a aucun) est gardé comme
    /// description, pour ne rien perdre à la réécriture. Pas de fichier : une description vide.
    pub fn lire(chemin: &Path) -> std::io::Result<Self> {
        let contenu = match std::fs::read_to_string(chemin) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let (mut titre, mut texte, mut mots) = (Vec::new(), Vec::new(), Vec::new());
        let mut courant = 1;
        for ligne in contenu.lines() {
            match ligne.trim() {
                "TITRE" => courant = 0,
                "DESCRIPTION" => courant = 1,
                "MOTS-CLÉS" | "MOTS-CLES" => courant = 2,
                _ => [&mut titre, &mut texte, &mut mots][courant].push(ligne),
            }
        }
        let joindre = |v: Vec<&str>| v.join("\n").trim().to_string();
        Ok(Self {
            titre: joindre(titre).split_whitespace().collect::<Vec<_>>().join(" "),
            texte: joindre(texte),
            mots_cles: normaliser_mots_cles(&joindre(mots)),
        })
    }

    pub fn ecrire(&self, chemin: &Path) -> std::io::Result<()> {
        let contenu = format!(
            "TITRE\n{}\n\nDESCRIPTION\n{}\n\nMOTS-CLÉS\n{}\n",
            self.titre.trim(),
            self.texte.trim(),
            normaliser_mots_cles(&self.mots_cles)
        );
        crate::ecriture::ecrire_atomique(chemin, "description", contenu.as_bytes())
    }
}

/// `str.capitalize` de Python : la première lettre en capitale, le reste en minuscules.
fn capitaliser(t: &str) -> String {
    let mut c = t.chars();
    c.next().map_or(String::new(), |p| p.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect())
}

/// Un premier jet tiré des textes de la vignette, à reprendre à la main.
pub fn description_proposee(f: &crate::fiches::Fiche) -> Description {
    let sujet = if f.sous_titre.is_empty() { capitaliser(&f.titre) } else { f.sous_titre.clone() };
    let titre = if !f.titre.is_empty() && !f.sous_titre.is_empty() {
        format!("VerdierCAM · {} : {}", capitaliser(&f.titre), f.sous_titre)
    } else if sujet.is_empty() {
        "VerdierCAM".to_string()
    } else {
        format!("VerdierCAM : {sujet}")
    };
    let puces: Vec<String> = [&f.ligne1, &f.ligne2, &f.ligne3].iter().filter(|l| !l.is_empty()).map(|l| format!("- {l}")).collect();
    let tete = if puces.is_empty() { String::new() } else { format!("{}\n\n", puces.join("\n")) };
    Description {
        titre,
        texte: format!("{tete}VerdierCAM : https://verdiercam.fr\nLe mode d'emploi : https://verdiercam.fr/mode-emploi/"),
        mots_cles: "VerdierCAM,CNC,FAO,G-code".into(),
    }
}

/// « a, b\nc » → « a,b,c » : une virgule entre les mots-clés, rien de vide.
pub fn normaliser_mots_cles(texte: &str) -> String {
    texte
        .replace('\n', ",")
        .split(',')
        .map(|m| m.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|m| !m.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fiches::Fiche;

    /// Les trois cas, tels que le Monteur 1 les donne.
    #[test]
    fn proposee_comme_le_monteur1() {
        let lien = "VerdierCAM : https://verdiercam.fr\nLe mode d'emploi : https://verdiercam.fr/mode-emploi/";
        let d = description_proposee(&Fiche {
            titre: "MACHINE".into(),
            sous_titre: "ajouter sa CNC".into(),
            ligne1: "Genmitsu".into(),
            ligne3: "usinage".into(),
            ..Fiche::default()
        });
        assert_eq!(d.titre, "VerdierCAM · Machine : ajouter sa CNC");
        assert_eq!(d.texte, format!("- Genmitsu\n- usinage\n\n{lien}"));
        assert_eq!(d.mots_cles, "VerdierCAM,CNC,FAO,G-code");
        assert_eq!(description_proposee(&Fiche { titre: "MACHINE".into(), ..Fiche::default() }).titre, "VerdierCAM : Machine");
        let vide = description_proposee(&Fiche::default());
        assert_eq!((vide.titre.as_str(), vide.texte.as_str()), ("VerdierCAM", lien));
    }
}
