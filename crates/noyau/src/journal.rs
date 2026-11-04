//! Le journal des gestes de VerdierCAM (`VCAM_JOURNAL_VIDEO`, format 1) : une ligne JSON par
//! geste, la première étant l'entête. C'est le contrat de VerdierCAM : les champs restent en
//! français et tels qu'il les écrit (`t`, `type`, `x`, `y`, `libelle`, `rect`…).

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, TimeZone};
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub struct Journal {
    pub chemin: PathBuf,
    pub entete: Map<String, Value>,
    /// Les lignes, dans l'ordre du temps, avec « t » en ms.
    pub evenements: Vec<Map<String, Value>>,
    /// L'heure murale du début (s depuis 1970).
    pub debut: f64,
}

impl Journal {
    /// Le journal, ou `None` s'il n'a pas d'entête lisible. Une ligne abîmée (VerdierCAM arrêté
    /// au milieu d'une écriture) est sautée.
    pub fn lire(chemin: &Path) -> Option<Self> {
        let octets = std::fs::read(chemin).ok()?;
        let texte = String::from_utf8_lossy(&octets);
        let mut entete = None;
        let mut evenements = Vec::new();
        for ligne in texte.lines() {
            let Ok(Value::Object(e)) = serde_json::from_str::<Value>(ligne) else { continue };
            if e.get("type").and_then(Value::as_str) == Some("entete") && entete.is_none() {
                entete = Some(e);
            } else if e.get("t").is_some_and(Value::is_number) {
                evenements.push(e);
            }
        }
        let entete = entete?;
        let debut = entete.get("debut_ms")?.as_f64()? / 1000.0;
        evenements.sort_by(|a, b| t(a).total_cmp(&t(b)));
        Some(Self { chemin: chemin.to_path_buf(), entete, evenements, debut })
    }

    /// Filmé par VerdierCAM lui-même (`--video`) : le temps du journal est celui du film.
    pub fn filme_par_verdiercam(&self) -> bool {
        self.entete.get("video").is_some_and(verite)
    }

    /// L'heure murale de la fin : la dernière ligne, ou la dernière écriture du fichier quand le
    /// temps du journal est celui du film (il avance image par image, plus lentement que
    /// l'horloge) : seule la date du fichier dit alors quand VerdierCAM a fini.
    pub fn fin(&self) -> f64 {
        let dernier = self.evenements.iter().map(t).fold(0.0, f64::max);
        let par_lignes = self.debut + dernier / 1000.0;
        if !self.filme_par_verdiercam() {
            return par_lignes;
        }
        let ecrit = std::fs::metadata(&self.chemin)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_secs_f64());
        par_lignes.max(ecrit)
    }

    pub fn taille_fenetre(&self) -> (i64, i64) {
        let f = self.entete.get("fenetre").and_then(Value::as_object);
        let px = |nom| f.and_then(|f| f.get(nom)).and_then(Value::as_f64).unwrap_or(0.0) as i64;
        (px("l_px"), px("h_px"))
    }

    /// Ce qu'il faut ajouter à un temps du journal (s depuis son début) pour avoir le temps de
    /// la vidéo.
    pub fn decalage(&self, video: &Path, duree: f64) -> f64 {
        if self.filme_par_verdiercam() {
            return 0.0; // t = 0 à la première image
        }
        self.debut - debut_de_la_video(video, duree)
    }
}

fn t(e: &Map<String, Value>) -> f64 {
    e.get("t").and_then(Value::as_f64).unwrap_or(0.0)
}

/// La vérité d'une valeur JSON comme la voit Python (`if entete.get("video")`).
fn verite(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// La date d'un nom d'OBS (« 2026-11-01 07-30-28 »), comme `_DATE_NOM` du Monteur 1 :
/// `AAAA-MM-JJ`, un séparateur parmi « _T », puis `HH`, « -h:. », `MM`, « -m:. », `SS`.
fn date_du_nom(nom: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let c: Vec<char> = nom.chars().collect();
    let chiffres = |i: usize, n: usize| -> Option<u32> {
        let s: String = c.get(i..i + n)?.iter().collect();
        if s.chars().all(|x| x.is_ascii_digit()) { s.parse().ok() } else { None }
    };
    let est = |i: usize, choix: &str| c.get(i).is_some_and(|x| choix.contains(*x));
    (0..c.len()).find_map(|i| {
        if est(i + 4, "-") && est(i + 7, "-") && est(i + 10, " _T") && est(i + 13, "-h:.") && est(i + 16, "-m:.") {
            Some((
                chiffres(i, 4)? as i32,
                chiffres(i + 5, 2)?,
                chiffres(i + 8, 2)?,
                chiffres(i + 11, 2)?,
                chiffres(i + 14, 2)?,
                chiffres(i + 17, 2)?,
            ))
        } else {
            None
        }
    })
}

/// L'heure murale du début de la vidéo : celle de son nom quand OBS l'a nommée, sinon sa date
/// de fin moins sa durée.
pub fn debut_de_la_video(video: &Path, duree: f64) -> f64 {
    let nom = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some((a, mo, j, h, mi, s)) = date_du_nom(&nom)
        && let Some(date) = NaiveDate::from_ymd_opt(a, mo, j).and_then(|d| d.and_hms_opt(h, mi, s))
        && let Some(locale) = Local.from_local_datetime(&date).earliest()
    {
        return locale.timestamp() as f64;
    }
    let fin = std::fs::metadata(video)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64());
    fin - duree
}

/// Le journal de gestes qui recouvre le plus la vidéo, cherché dans `dossiers` (à côté de la
/// vidéo et dans `~/.local/share/monteur/gestes` pour l'appli).
pub fn trouver_journal(video: &Path, duree: f64, dossiers: &[PathBuf]) -> Option<Journal> {
    let v0 = debut_de_la_video(video, duree);
    let v1 = v0 + duree;
    let mut meilleur: Option<(Journal, f64)> = None;
    for d in dossiers {
        let Ok(entrees) = std::fs::read_dir(d) else { continue };
        let mut chemins: Vec<PathBuf> = entrees
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("gestes ") && n.ends_with(".jsonl"))
            })
            .collect();
        chemins.sort();
        for chemin in chemins {
            let Some(j) = Journal::lire(&chemin) else { continue };
            let commun = v1.min(j.fin()) - v0.max(j.debut);
            if commun > meilleur.as_ref().map_or(0.0, |m| m.1) {
                meilleur = Some((j, commun));
            }
        }
    }
    meilleur.map(|m| m.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_d_un_nom_d_obs() {
        assert_eq!(date_du_nom("2026-11-01 07-30-28"), Some((2026, 11, 1, 7, 30, 28)));
        assert_eq!(date_du_nom("prise 2026-11-01_07h30m28 bis"), Some((2026, 11, 1, 7, 30, 28)));
        assert_eq!(date_du_nom("vacances"), None);
        assert_eq!(date_du_nom("2026-11-01 07-30"), None);
    }
}
