//! Le noyau Rust contre le Monteur 1, sur de vrais fichiers de Christophe (`tests/donnees/`) :
//! `outils/reference_python.py` y a fait calculer au Monteur 1 ce qu'il lit et calcule ; le noyau
//! doit retrouver la même chose. Les fichiers de données ne sont que lus ; les réécritures vont dans
//! un dossier jetable.

use std::path::{Path, PathBuf};

use noyau::description::Description;
use noyau::fiches::Fiches;
use noyau::journal::{self, Journal};
use noyau::montage::{self, Montage};
use noyau::projet::Projet;
use serde_json::Value;

fn donnees() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees")
}

fn reference() -> Value {
    let texte = std::fs::read_to_string(donnees().join("reference.json")).expect("reference.json");
    serde_json::from_str(&texte).unwrap()
}

fn jetable(nom: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("monteur-noyau-{nom}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Égalité de deux valeurs JSON, les nombres à 1e-9 près (3 et 3.0 sont égaux).
fn egal(a: &Value, b: &Value, ou: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs())) {
                Ok(())
            } else {
                Err(format!("{ou} : {x} au lieu de {y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{ou} : {} éléments au lieu de {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().try_for_each(|(i, (p, q))| egal(p, q, &format!("{ou}[{i}]")))
        }
        (Value::Object(x), Value::Object(y)) => {
            let mut cles: Vec<&String> = x.keys().chain(y.keys()).collect();
            cles.sort();
            cles.dedup();
            cles.into_iter().try_for_each(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => egal(p, q, &format!("{ou}.{k}")),
                (Some(_), None) => Err(format!("{ou}.{k} : en trop")),
                (None, _) => Err(format!("{ou}.{k} : manque")),
            })
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{ou} : {a} au lieu de {b}")),
    }
}

fn verifier(a: Value, b: &Value, ou: &str) {
    if let Err(e) = egal(&a, b, ou) {
        panic!("{e}");
    }
}

fn liste<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap()
}

#[test]
fn montages_lus_et_calcules_comme_le_monteur1() {
    let r = reference();
    let montages = r["montages"].as_object().unwrap();
    assert!(montages.len() >= 2);
    for (nom, attendu) in montages {
        let m = Montage::lire(&donnees().join(nom)).unwrap_or_else(|| panic!("{nom} illisible"));
        verifier(m.en_valeur(), &attendu["reecrit"], &format!("{nom} réécrit"));
        assert_eq!(Value::Bool(m.vide()), attendu["vide"], "{nom} vide");
        verifier(liste(m.coupes_actives()), &attendu["coupes_actives"], &format!("{nom} coupes"));
        verifier(liste(m.accelerations()), &attendu["accelerations"], &format!("{nom} accélérations"));
        verifier(liste(m.duree_montee()), &attendu["duree_montee"], &format!("{nom} durée montée"));
        assert_eq!(Value::Bool(m.sous_titres_a_part()), attendu["sous_titres_a_part"]);

        let instants: Vec<f64> = serde_json::from_value(attendu["instants"].clone()).unwrap();
        let cles = montage::cles_zoom(&m.zooms);
        let cles_val: Vec<(f64, f64, f64, f64)> = cles.iter().map(|c| (c.t, c.facteur, c.cx, c.cy)).collect();
        verifier(liste(&cles_val), &attendu["cles"], &format!("{nom} clés du zoom"));
        let etats: Vec<_> = instants.iter().map(|&t| montage::etat_zoom(&cles, t)).collect();
        verifier(liste(&etats), &attendu["etats"], &format!("{nom} zoom"));
        let rects: Vec<_> = etats
            .iter()
            .map(|&(f, x, y)| montage::rect_source(f, x, y, m.largeur as f64, m.hauteur as f64))
            .collect();
        verifier(liste(&rects), &attendu["rects"], &format!("{nom} cadre source"));
        let vitesses: Vec<_> = instants.iter().map(|&t| m.vitesse_a(t)).collect();
        verifier(liste(&vitesses), &attendu["vitesses"], &format!("{nom} vitesses"));
        let dans: Vec<_> = instants.iter().map(|&t| m.dans_une_coupe(t)).collect();
        verifier(liste(&dans), &attendu["dans_une_coupe"], &format!("{nom} dans une coupe"));
        let coupes = m.coupes_actives();
        let montes: Vec<_> = instants.iter().map(|&t| montage::temps_monte(&coupes, t)).collect();
        verifier(liste(&montes), &attendu["temps_montes"], &format!("{nom} temps montés"));
    }
}

#[test]
fn montage_reecrit_sans_rien_perdre() {
    let r = reference();
    let d = jetable("montage");
    for nom in ["2026-11-02 10-06-06 montage.json", "2026-11-04 07-59-21 montage.json"] {
        // Ce que le Monteur 1 réécrit de ce fichier (il complète les champs absents, comme
        // `vitesse` des coupes d'avant le 02/11) : c'est la référence, pas le fichier brut.
        let reecrit_python = &r["montages"][nom]["reecrit"];
        let original: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join(nom)).unwrap()).unwrap();
        // Un champ qu'un Monteur plus récent aurait ajouté, partout : il doit survivre.
        let mut enrichi = original.clone();
        enrichi["piste_musique"] = Value::from("Séquence Oxygène");
        enrichi["zooms"][0]["courbe"] = Value::from("douce");
        enrichi["reglages"]["niveau_lufs"] = Value::from(-14.0);
        let chemin = d.join(nom);
        std::fs::write(&chemin, enrichi.to_string()).unwrap();

        let mut m = Montage::lire(&chemin).unwrap();
        m.ecrire(&chemin).unwrap();
        let relu: Value = serde_json::from_str(&std::fs::read_to_string(&chemin).unwrap()).unwrap();
        assert_eq!(relu["piste_musique"], "Séquence Oxygène");
        assert_eq!(relu["zooms"][0]["courbe"], "douce");
        assert_eq!(relu["reglages"]["niveau_lufs"], -14.0);
        // Et le reste, comme le Monteur 1 l'aurait réécrit.
        let mut sans = relu.clone();
        sans.as_object_mut().unwrap().remove("piste_musique");
        sans["zooms"][0].as_object_mut().unwrap().remove("courbe");
        sans["reglages"].as_object_mut().unwrap().remove("niveau_lufs");
        verifier(sans, reecrit_python, nom);
        let _ = &original;
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn element_illisible_saute_comme_le_monteur1() {
    let texte = r#"{"format": 1, "duree": 10, "zooms": [{"debut": 1, "fin": 2}, {"debut": "x"}, 3,
        {"fin": 4}], "coupes": [{"debut": 0, "fin": 1}], "reglages": "abîmé"}"#;
    let m = Montage::depuis_valeur(serde_json::from_str(texte).unwrap()).unwrap();
    assert_eq!(m.zooms.len(), 1);
    assert_eq!(m.zooms[0].facteur, 1.8);
    assert_eq!(m.coupes[0].vitesse, 0.0);
    assert!(m.coupes[0].active);
    assert_eq!(m.reglages, montage::Reglages::default());
    assert!(Montage::depuis_valeur(serde_json::json!({"format": 2})).is_none());
    assert!(Montage::depuis_valeur(serde_json::json!({"format": 1, "duree": "long"})).is_none());
}

#[test]
fn journal_lu_comme_le_monteur1() {
    let r = reference();
    for (nom, attendu) in r["journal"].as_object().unwrap() {
        let j = Journal::lire(&donnees().join(nom)).unwrap();
        verifier(liste(j.debut), &attendu["debut"], "début");
        assert_eq!(j.evenements.len() as u64, attendu["nb"].as_u64().unwrap());
        verifier(liste(&j.evenements[..5]), &attendu["premiers"], "premières lignes");
        verifier(liste(&j.evenements[j.evenements.len() - 3..]), &attendu["derniers"], "dernières lignes");
        let (l, h) = j.taille_fenetre();
        verifier(liste([l, h]), &attendu["taille_fenetre"], "fenêtre");
        let video = donnees().join("2026-11-02 10-06-06.mp4");
        verifier(liste(journal::debut_de_la_video(&video, 94.6)), &attendu["debut_video"], "début de la vidéo");
        verifier(liste(j.decalage(&video, 94.6)), &attendu["decalage"], "décalage");
        let trouve = journal::trouver_journal(&video, 94.6, &[donnees()]).unwrap();
        assert_eq!(trouve.chemin.file_name().unwrap().to_str(), attendu["trouve"].as_str());
    }
}

#[test]
fn journal_abime_ou_sans_entete() {
    let d = jetable("journal");
    let chemin = d.join("gestes 2026-11-02 10-05-56.jsonl");
    std::fs::write(&chemin, "{\"t\": 5, \"type\": \"clic\"}\n{abîmé\n[1,2]\n").unwrap();
    assert!(Journal::lire(&chemin).is_none(), "sans entête, pas de journal");
    std::fs::write(
        &chemin,
        "{\"type\":\"entete\",\"debut_ms\":1000}\n{\"t\": 9, \"type\": \"clic\"}\n{\"t\": 3, \"ty\n{\"t\": 2}\n",
    )
    .unwrap();
    let j = Journal::lire(&chemin).unwrap();
    assert_eq!(j.evenements.len(), 2, "la ligne coupée est sautée");
    assert_eq!(j.evenements[0]["t"], 2, "dans l'ordre du temps");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn projet_lu_comme_le_monteur1_et_reecrit() {
    let r = reference();
    let d = jetable("projet");
    for (nom, attendu) in r["projet"].as_object().unwrap() {
        let p = Projet::lire(&donnees().join(nom)).unwrap();
        assert_eq!(p.video.to_str(), attendu["video"].as_str());
        assert_eq!(p.taille, attendu["taille"].as_u64().unwrap());
        assert_eq!(p.enregistre_le, attendu["enregistre_le"].as_str().unwrap());
        verifier(liste(&p.fiche), &attendu["fiche"], "fiche du projet");
        verifier(liste(&p.description), &attendu["description"], "description du projet");
        verifier(p.montage().unwrap().en_valeur(), &attendu["montage"], "montage du projet");

        let copie = d.join(nom);
        p.ecrire(&copie, "2.0.0-essai").unwrap();
        let relu = Projet::lire(&copie).unwrap();
        assert_eq!(relu.enregistre_le, p.enregistre_le);
        verifier(relu.montage.clone().unwrap(), p.montage.as_ref().unwrap(), "montage réécrit");
        verifier(liste(&relu.description), &attendu["description"], "description réécrite");
    }
    assert!(Projet::lire(&donnees().join("fiches.json")).is_err(), "des fiches ne sont pas un projet");
    std::fs::write(d.join("futur.monteur"), r#"{"format": 2, "logiciel": "Monteur", "video": "/x.mp4"}"#).unwrap();
    let e = Projet::lire(&d.join("futur.monteur")).unwrap_err();
    assert!(e.0.contains("plus récent"), "{e}");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn fiches_lues_comme_le_monteur1_et_reecrites() {
    let r = reference();
    let f = Fiches::lire(&donnees().join("fiches.json")).unwrap();
    let attendu = &r["fiches"];
    assert_eq!(f.dossier.as_deref().unwrap().to_str(), attendu["dossier"].as_str());
    assert_eq!(f.rangement.as_deref().unwrap().to_str(), attendu["rangement"].as_str());
    verifier(liste(&f.fiches), &attendu["fiches"], "fiches");

    let d = jetable("fiches");
    let copie = d.join("fiches.json");
    f.ecrire(&copie, Path::new("/nulle/part"), Path::new("/nulle/part")).unwrap();
    let original: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join("fiches.json")).unwrap()).unwrap();
    let relu: Value = serde_json::from_str(&std::fs::read_to_string(&copie).unwrap()).unwrap();
    verifier(relu.clone(), &original, "fiches réécrites");
    // Les cadres restent des entiers : le Monteur 1 les relit tels quels.
    assert_eq!(relu.to_string().contains(".0,"), original.to_string().contains(".0,"));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn description_lue_comme_le_monteur1() {
    let r = reference();
    for (nom, attendu) in r["description"].as_object().unwrap() {
        let d = Description::lire(&donnees().join(nom)).unwrap();
        verifier(liste(&d), attendu, nom);
    }
    assert!(Description::lire(&donnees().join("absente.txt")).unwrap().vide());
}
