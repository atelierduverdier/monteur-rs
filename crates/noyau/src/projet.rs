//! Le projet (`.monteur`) : le montage, la vignette et la description d'une vidéo dans un seul
//! fichier qu'on range où l'on veut. Il ne contient pas la vidéo, seulement son chemin et sa
//! taille, pour la reconnaître si elle a bougé (`noyau.Projet` du Monteur 1).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::Erreur;
use crate::description::Description;
use crate::ecriture;
use crate::fiches::Fiche;
use crate::montage::Montage;

pub const FORMAT_PROJET: i64 = 1;

#[derive(Debug, Clone, Default)]
pub struct Projet {
    pub video: PathBuf,
    pub fiche: Fiche,
    pub description: Description,
    /// Le montage tel qu'écrit ; `montage()` le relit.
    pub montage: Option<Value>,
    /// Octets de la vidéo.
    pub taille: u64,
    pub enregistre_le: String,
    pub autres: Map<String, Value>,
}

impl Projet {
    pub fn montage(&self) -> Option<Montage> {
        self.montage.clone().and_then(Montage::depuis_valeur)
    }

    pub fn lire(chemin: &Path) -> Result<Self, Erreur> {
        let texte = std::fs::read_to_string(chemin).map_err(|e| Erreur(format!("fichier illisible ({e})")))?;
        let valeur: Value = serde_json::from_str(&texte).map_err(|e| Erreur(format!("fichier illisible ({e})")))?;
        let Value::Object(mut d) = valeur else {
            return Err(Erreur("ce n'est pas un projet du Monteur".into()));
        };
        if d.get("logiciel").and_then(Value::as_str) != Some("Monteur") || !d.get("video").is_some_and(Value::is_string) {
            return Err(Erreur("ce n'est pas un projet du Monteur".into()));
        }
        match d.get("format").and_then(Value::as_i64) {
            Some(f) if f <= FORMAT_PROJET && d["format"].is_i64() => {}
            _ => return Err(Erreur("projet d'un Monteur plus récent : mettez le Monteur à jour".into())),
        }
        for cle in ["format", "logiciel", "version"] {
            d.remove(cle);
        }
        let video = PathBuf::from(d.remove("video").and_then(|v| v.as_str().map(String::from)).unwrap_or_default());
        let description = match d.remove("description") {
            Some(Value::Object(m)) => {
                let champ = |nom: &str| m.get(nom).and_then(Value::as_str).unwrap_or_default().to_string();
                Description { titre: champ("titre"), texte: champ("texte"), mots_cles: champ("mots_cles") }
            }
            _ => Description::default(),
        };
        let fiche = match d.remove("fiche") {
            Some(Value::Object(m)) => Fiche::depuis(m),
            _ => Fiche::default(),
        };
        let montage = match d.remove("montage") {
            Some(m @ Value::Object(_)) => Some(m),
            _ => None,
        };
        let taille = d.remove("taille").and_then(|v| v.as_u64()).unwrap_or(0);
        let enregistre_le = match d.remove("enregistre_le") {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        Ok(Self { video, fiche, description, montage, taille, enregistre_le, autres: d })
    }

    /// `version` est celle du logiciel qui écrit ; `enregistre_le`, s'il est vide, est l'heure
    /// qu'il est.
    pub fn ecrire(&self, chemin: &Path, version: &str) -> std::io::Result<()> {
        let video = std::path::absolute(&self.video).unwrap_or_else(|_| self.video.clone());
        let video = video.canonicalize().unwrap_or(video);
        let enregistre_le = if self.enregistre_le.is_empty() {
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            self.enregistre_le.clone()
        };
        // La date de la vignette ne vaut que sur ce poste, pour ce fichier : elle ne voyage pas.
        let mut fiche = serde_json::to_value(&self.fiche).expect("une fiche s'écrit toujours");
        fiche["vignette_ns"] = Value::from(0);
        let mut d = Map::new();
        d.insert("format".into(), Value::from(FORMAT_PROJET));
        d.insert("logiciel".into(), "Monteur".into());
        d.insert("version".into(), version.into());
        d.insert("enregistre_le".into(), enregistre_le.into());
        d.insert("video".into(), video.to_string_lossy().into_owned().into());
        d.insert("taille".into(), Value::from(self.taille));
        d.insert("fiche".into(), fiche);
        d.insert("description".into(), serde_json::to_value(&self.description).expect("description"));
        d.insert("montage".into(), self.montage.clone().unwrap_or(Value::Null));
        for (k, v) in &self.autres {
            d.entry(k.clone()).or_insert_with(|| v.clone());
        }
        ecriture::ecrire_json(chemin, "projet", &d)
    }

    /// La vidéo du projet : à son chemin, sinon du même nom à côté du projet ou dans l'un des
    /// `dossiers`. Celle qui a la même taille d'abord ; `None` si elle n'est nulle part.
    pub fn retrouver_video(&self, chemin_projet: &Path, dossiers: &[PathBuf]) -> Option<PathBuf> {
        let nom = self.video.file_name()?;
        let mut candidats = vec![self.video.clone(), chemin_projet.parent().unwrap_or(Path::new(".")).join(nom)];
        candidats.extend(dossiers.iter().map(|d| d.join(nom)));
        let mut existants: Vec<PathBuf> = Vec::new();
        for c in candidats {
            let Ok(c) = c.canonicalize() else { continue };
            if c.is_file() && !existants.contains(&c) {
                existants.push(c);
            }
        }
        if self.taille > 0
            && let Some(meme) = existants
                .iter()
                .find(|c| std::fs::metadata(c).map(|m| m.len()).ok() == Some(self.taille))
        {
            return Some(meme.clone());
        }
        existants.into_iter().next()
    }
}
