//! Les fiches de toutes les vidéos (`~/.local/share/monteur/fiches.json`) : textes de la vignette,
//! image choisie, état d'envoi. Même format que le Monteur 1 (`noyau.Registre`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Number, Value};

use crate::Erreur;
use crate::ecriture;

pub const CHAMPS: [&str; 5] = ["titre", "sous_titre", "ligne1", "ligne2", "ligne3"];

/// Ce que le Monteur retient d'une vidéo.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Fiche {
    pub titre: String,
    pub sous_titre: String,
    pub ligne1: String,
    pub ligne2: String,
    pub ligne3: String,
    /// Seconde de la vidéo prise pour l'image.
    pub temps: f64,
    /// x, y, largeur, hauteur en pixels de la vidéo. Les nombres restent tels qu'écrits (entiers
    /// le plus souvent) : le Monteur 1 relit ce fichier.
    pub cadre: Option<Vec<Number>>,
    /// Pas une vidéo pour YouTube.
    pub ignoree: bool,
    /// Date (st_mtime_ns) de la vignette que le Monteur a posée.
    pub vignette_ns: i64,
    pub youtube_id: String,
    pub envoyee_le: String,
    #[serde(flatten)]
    pub autres: Map<String, Value>,
}

impl Fiche {
    /// Une fiche lue du JSON ; une valeur du mauvais type (fichier retouché à la main) reprend la
    /// valeur par défaut (`Fiche.depuis`).
    pub fn depuis(mut d: Map<String, Value>) -> Self {
        let mut f = Fiche::default();
        let mut texte = |nom: &str, cible: &mut String| {
            if let Some(Value::String(s)) = d.remove(nom) {
                *cible = s;
            }
        };
        texte("titre", &mut f.titre);
        texte("sous_titre", &mut f.sous_titre);
        texte("ligne1", &mut f.ligne1);
        texte("ligne2", &mut f.ligne2);
        texte("ligne3", &mut f.ligne3);
        texte("youtube_id", &mut f.youtube_id);
        texte("envoyee_le", &mut f.envoyee_le);
        if let Some(Value::Number(n)) = d.remove("temps") {
            f.temps = n.as_f64().unwrap_or(0.0);
        }
        if let Some(Value::Array(c)) = d.remove("cadre")
            && c.len() == 4
            && c.iter().all(Value::is_number)
        {
            f.cadre = Some(c.into_iter().filter_map(|v| v.as_number().cloned()).collect());
        }
        if let Some(Value::Bool(b)) = d.remove("ignoree") {
            f.ignoree = b;
        }
        if let Some(Value::Number(n)) = d.remove("vignette_ns") {
            f.vignette_ns = n.as_i64().or_else(|| n.as_f64().map(|x| x as i64)).unwrap_or(0);
        }
        f.autres = d;
        f
    }

    pub fn est_vide(&self) -> bool {
        *self == Fiche::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Fiches {
    pub dossier: Option<PathBuf>,
    pub rangement: Option<PathBuf>,
    /// Le dossier du dernier projet ouvert ou enregistré.
    pub projets: Option<PathBuf>,
    /// Par chemin absolu de la vidéo.
    pub fiches: BTreeMap<String, Fiche>,
    pub autres: Map<String, Value>,
}

impl Fiches {
    /// Pas de fichier : des fiches vides. Un fichier illisible est une erreur : l'appelant le met
    /// de côté, il ne l'écrase jamais.
    pub fn lire(chemin: &Path) -> Result<Self, Erreur> {
        let texte = match std::fs::read_to_string(chemin) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Erreur(format!("fiches illisibles ({e})"))),
        };
        let Ok(Value::Object(mut d)) = serde_json::from_str::<Value>(&texte) else {
            return Err(Erreur("ce n'est pas un fichier de fiches".into()));
        };
        let fiches = match d.remove("fiches") {
            None => Map::new(),
            Some(Value::Object(f)) => f,
            Some(_) => return Err(Erreur("ce n'est pas un fichier de fiches".into())),
        };
        let mut chemin_de = |nom: &str| match d.remove(nom) {
            Some(Value::String(s)) if !s.is_empty() => Some(PathBuf::from(s)),
            _ => None,
        };
        let (dossier, rangement, projets) = (chemin_de("dossier"), chemin_de("rangement"), chemin_de("projets"));
        d.remove("version");
        Ok(Self {
            dossier,
            rangement,
            projets,
            fiches: fiches
                .into_iter()
                .filter_map(|(nom, f)| match f {
                    Value::Object(m) => Some((nom, Fiche::depuis(m))),
                    _ => None,
                })
                .collect(),
            autres: d,
        })
    }

    /// Le dossier et le rangement prennent ceux du Monteur 1 par défaut quand ils manquent.
    pub fn ecrire(&self, chemin: &Path, dossier_defaut: &Path, rangement_defaut: &Path) -> std::io::Result<()> {
        let texte = |p: &Option<PathBuf>, defaut: &Path| {
            Value::String(p.as_deref().unwrap_or(defaut).to_string_lossy().into_owned())
        };
        let mut d = Map::new();
        d.insert("version".into(), Value::from(1));
        d.insert("dossier".into(), texte(&self.dossier, dossier_defaut));
        d.insert("rangement".into(), texte(&self.rangement, rangement_defaut));
        d.insert(
            "projets".into(),
            Value::String(self.projets.as_deref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
        );
        // Une fiche restée vide n'apprend rien : inutile de la garder.
        let fiches: Map<String, Value> = self
            .fiches
            .iter()
            .filter(|(_, f)| !f.est_vide())
            .map(|(nom, f)| (nom.clone(), serde_json::to_value(f).expect("une fiche s'écrit toujours")))
            .collect();
        d.insert("fiches".into(), Value::Object(fiches));
        for (k, v) in &self.autres {
            d.entry(k.clone()).or_insert_with(|| v.clone());
        }
        ecriture::ecrire_json(chemin, "fiches", &d)
    }
}
