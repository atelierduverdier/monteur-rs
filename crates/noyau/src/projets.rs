//! Un dossier par projet (Christophe, 05/11/2026 : « que chaque projet ait son dossier, afin de pas
//! tout mélanger ») : `~/Vidéos/Projets/<nom>/` tient la prise, son montage, ses vignettes, ses
//! sous-titres et les vidéos qu'on y importe. La prise y est déplacée, pas copiée. Comme tout s'écrit
//! à côté de la vidéo, un projet est simplement un dossier : le Monteur 1 l'ouvre aussi.

use std::path::{Path, PathBuf};

use crate::Erreur;
use crate::fiches::Fiches;
use crate::videos;

/// Les vidéos importées ont leur sous-dossier : elles ne sont pas des prises de la liste.
pub const IMPORTS: &str = "Vidéos importées";

pub fn racine() -> PathBuf {
    std::env::home_dir().unwrap_or_default().join("Vidéos").join("Projets")
}

/// Un nom de dossier sûr d'après ce qu'on a tapé ; `None` s'il n'en reste rien.
pub fn nom_de_projet(texte: &str) -> Option<String> {
    let propre: String = texte.chars().map(|c| if c == '/' || c.is_control() { ' ' } else { c }).collect();
    let propre = propre.split_whitespace().collect::<Vec<_>>().join(" ");
    let propre = propre.trim_start_matches('.').trim().to_string();
    (!propre.is_empty()).then_some(propre)
}

/// Les projets, le plus récemment touché d'abord.
pub fn lister(racine: &Path) -> Vec<PathBuf> {
    let Ok(entrees) = std::fs::read_dir(racine) else { return Vec::new() };
    let mut p: Vec<PathBuf> = entrees
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .collect();
    let date = |p: &PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    p.sort_by_key(|x| std::cmp::Reverse(date(x)));
    p
}

/// Déplace la prise, ce qui l'accompagne et sa fiche dans le projet `nom`, créé s'il n'existe pas.
/// Rend le nouveau chemin de la vidéo. Rien n'est écrasé (`videos::deplacer`).
pub fn ranger_dans_un_projet(
    fiches: &mut Fiches,
    chemin_fiches: &Path,
    video: &Path,
    racine: &Path,
    nom: &str,
) -> Result<PathBuf, Erreur> {
    let nom = nom_de_projet(nom).ok_or_else(|| Erreur("Ce nom de projet est vide.".into()))?;
    videos::deplacer(fiches, chemin_fiches, video, &racine.join(nom))
}

/// Copie une vidéo dans le projet ; rend son chemin relatif au dossier du projet (celui que garde
/// le montage). Déjà là à l'identique : elle n'est pas recopiée ; un autre fichier du même nom :
/// on numérote, rien n'est écrasé. La vidéo d'origine n'est pas touchée.
pub fn importer(source: &Path, dossier: &Path) -> Result<String, Erreur> {
    let err = |e: std::io::Error| Erreur(format!("{} : {e}", source.display()));
    let taille = std::fs::metadata(source).map_err(err)?.len();
    let imports = dossier.join(IMPORTS);
    std::fs::create_dir_all(&imports).map_err(err)?;
    let nom = source.file_name().ok_or_else(|| Erreur("Ce n'est pas un fichier.".into()))?.to_string_lossy().into_owned();
    let (base, ext) = match nom.rsplit_once('.') {
        Some((b, e)) => (b.to_string(), format!(".{e}")),
        None => (nom.clone(), String::new()),
    };
    let mut rang = 1;
    let cible = loop {
        let essai = if rang == 1 { imports.join(&nom) } else { imports.join(format!("{base} ({rang}){ext}")) };
        match std::fs::metadata(&essai) {
            Err(_) => break essai,
            Ok(m) if m.len() == taille => {
                return Ok(format!("{IMPORTS}/{}", essai.file_name().unwrap_or_default().to_string_lossy()));
            }
            Ok(_) => rang += 1,
        }
    };
    let partiel = videos::chemin_partiel(&cible);
    std::fs::copy(source, &partiel).map_err(err)?;
    std::fs::rename(&partiel, &cible).map_err(err)?;
    Ok(format!("{IMPORTS}/{}", cible.file_name().unwrap_or_default().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jetable(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("monteur-projets-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn noms_surs() {
        assert_eq!(nom_de_projet("  Graver  une image ").as_deref(), Some("Graver une image"));
        assert_eq!(nom_de_projet("a/b").as_deref(), Some("a b"));
        assert_eq!(nom_de_projet("../secret").as_deref(), Some("secret"));
        assert_eq!(nom_de_projet("   "), None);
        assert_eq!(nom_de_projet(".."), None);
    }

    #[test]
    fn la_prise_part_dans_son_projet_avec_tout_le_reste() {
        let d = jetable("ranger");
        let (copies, projets) = (d.join("Copies"), d.join("Projets"));
        std::fs::create_dir_all(&copies).unwrap();
        let video = copies.join("x.mp4");
        for n in ["x.mp4", "x montage.json", "x vignette YouTube.png", "autre.mp4"] {
            std::fs::write(copies.join(n), n).unwrap();
        }
        let mut fiches = Fiches::default();
        let mut f = crate::fiches::Fiche::default();
        f.titre = "GRAVER".into();
        fiches.fiches.insert(videos::cle(&video), f);
        let chemin_fiches = d.join("fiches.json");
        let nouvelle = ranger_dans_un_projet(&mut fiches, &chemin_fiches, &video, &projets, "Graver une image").unwrap();
        assert!(nouvelle.ends_with("Graver une image/x.mp4") && nouvelle.exists());
        assert!(projets.join("Graver une image/x montage.json").exists());
        assert!(projets.join("Graver une image/x vignette YouTube.png").exists());
        assert!(!video.exists(), "déplacée, pas copiée");
        assert!(copies.join("autre.mp4").exists(), "le reste ne bouge pas");
        assert_eq!(fiches.fiches[&videos::cle(&nouvelle)].titre, "GRAVER", "la fiche suit");
        assert_eq!(lister(&projets).len(), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn importer_sans_jamais_ecraser() {
        let d = jetable("importer");
        let source = d.join("atelier.mp4");
        std::fs::write(&source, "abc").unwrap();
        let projet = d.join("Projet");
        assert_eq!(importer(&source, &projet).unwrap(), "Vidéos importées/atelier.mp4");
        assert!(source.exists(), "l'originale reste");
        // Le même fichier : pas de seconde copie.
        assert_eq!(importer(&source, &projet).unwrap(), "Vidéos importées/atelier.mp4");
        // Un autre du même nom : numéroté.
        std::fs::write(&source, "abcdef").unwrap();
        assert_eq!(importer(&source, &projet).unwrap(), "Vidéos importées/atelier (2).mp4");
        assert_eq!(std::fs::read_to_string(projet.join("Vidéos importées/atelier.mp4")).unwrap(), "abc");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
