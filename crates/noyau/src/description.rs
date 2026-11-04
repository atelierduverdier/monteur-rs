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
