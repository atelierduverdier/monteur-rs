//! « Préparer le montage » du noyau Rust contre celui du Monteur 1, sur la prise du 02/11/2026 à
//! 10:06 et son journal : même calage, mêmes zooms, étiquettes, clics et temps morts ; puis les
//! sous-titres et le .srt. Le mouvement de l'image et les silences ont été mesurés par ffmpeg sur la
//! vraie vidéo et gardés dans `reference.json` (`outils/reference_python.py`).

use std::path::{Path, PathBuf};

use noyau::analyse;
use noyau::journal::Journal;
use noyau::montage::Montage;
use noyau::sous_titres;
use noyau::temps_morts;
use serde_json::Value;

fn donnees() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees")
}

fn reference() -> Value {
    serde_json::from_str(&std::fs::read_to_string(donnees().join("reference.json")).unwrap()).unwrap()
}

fn egal(a: &Value, b: &Value, ou: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() <= 1e-9 * (1.0 + x.abs().max(y.abs())) { Ok(()) } else { Err(format!("{ou} : {x} au lieu de {y}")) }
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

fn verifier<T: serde::Serialize>(a: T, b: &Value, ou: &str) {
    if let Err(e) = egal(&serde_json::to_value(a).unwrap(), b, ou) {
        panic!("{e}");
    }
}

fn nombres(v: &Value) -> Vec<f64> {
    serde_json::from_value(v.clone()).unwrap()
}

#[test]
fn preparation_comme_le_monteur1() {
    let r = reference();
    let preparations = r["preparations"].as_object().unwrap();
    assert_eq!(preparations.len(), 2, "reference.json fait sans les vraies prises : relancer outils/reference_python.py");
    for (nom, p) in preparations {
        preparer(nom, p);
    }
}

fn preparer(nom: &str, p: &Value) {
    let video = donnees().join(format!("{nom}.mp4")); // son nom porte l'heure ; elle n'a pas à être là
    let depart = Montage::lire(&donnees().join(format!("{nom} montage.json"))).unwrap();
    let mut m = depart.clone();
    let mouvement = nombres(&p["mouvement"]);
    let j = Journal::lire(&donnees().join(p["journal"].as_str().unwrap())).unwrap();

    // Le calage : les clics là où l'écran change.
    let dec0 = j.decalage(&video, m.duree);
    let clics = analyse::instants_actifs(&j, dec0, true);
    verifier(&clics, &p["clics"], &format!("{nom} instants des clics"));
    let trouve = analyse::calage_automatique(&mouvement, &clics, 1.5, 0.05);
    verifier(trouve, &p["calage"], &format!("{nom} calage"));
    if let Some(t) = trouve
        && m.reglages.calage_auto
    {
        m.reglages.decalage_journal = t;
    }
    verifier(analyse::changements_d_ecran(&mouvement), &p["changements"], &format!("{nom} changements d'écran"));

    analyse::analyser_journal(&j, &mut m, &video, Some(&mouvement));
    analyse::caler_etiquettes_sur_la_voix(&mut m);

    // Les temps morts.
    let silences = temps_morts::lire_silences(p["silences_txt"].as_str().unwrap(), m.duree);
    verifier(&silences, &p["silences"], &format!("{nom} silences"));
    let dec = j.decalage(&video, m.duree) + m.reglages.decalage_journal;
    let actifs = analyse::instants_actifs(&j, dec, false);
    verifier(&actifs, &p["actifs"], &format!("{nom} instants actifs"));
    let nouvelles = temps_morts::temps_morts(&silences, Some(&actifs), Some(&mouvement));
    m.coupes.retain(|c| c.origine != "temps mort");
    m.coupes.extend(nouvelles);
    m.trier();
    verifier(m.en_valeur(), &p["montage"], &format!("{nom} montage préparé"));

    verifier(temps_morts::temps_morts(&silences, None, Some(&mouvement)), &p["sans_journal"], &format!("{nom} temps morts sans journal"));

    let mut sans = depart.clone();
    analyse::analyser_journal(&j, &mut sans, &video, None);
    verifier(sans.en_valeur(), &p["sans_mouvement"], &format!("{nom} gestes sans mouvement"));
}

#[test]
fn sous_titres_et_srt_comme_le_monteur1() {
    let r = reference();
    for (nom, attendu) in r["srt"].as_object().unwrap() {
        let m = Montage::lire(&donnees().join(nom)).unwrap();
        assert_eq!(sous_titres::texte_srt(&m, 0.0), attendu["texte"].as_str().unwrap(), "{nom} .srt");
        assert_eq!(sous_titres::texte_srt(&m, 3.0), attendu["decale"].as_str().unwrap(), "{nom} .srt décalé");
        let decoupes: Vec<_> = m.sous_titres.iter().map(|s| sous_titres::decouper_sous_titre(&s.texte)).collect();
        verifier(&decoupes, &attendu["decoupes"], &format!("{nom} découpes"));
        verifier(sous_titres::fusionner_sous_titres(m.sous_titres.clone()), &attendu["fusion"], &format!("{nom} fusion"));
        verifier(sous_titres::son_retire(&m), &attendu["son_retire"], &format!("{nom} son retiré"));
        let gardees: Vec<bool> = (0..(m.duree * 30.0) as i64).map(|k| sous_titres::image_gardee(&m, k as f64 / 30.0, 30.0)).collect();
        verifier(&gardees, &attendu["gardees"], &format!("{nom} images gardées"));
    }
    let whisper: Value = serde_json::from_str(&std::fs::read_to_string(donnees().join("whisper-essai.json")).unwrap()).unwrap();
    verifier(sous_titres::lire_whisper(&whisper), &r["whisper"], "transcription de whisper");
}

#[test]
fn mouvement_d_images_grises() {
    let taille = analyse::GRIS_L * analyse::GRIS_H;
    let mut octets = vec![10u8; taille * 3];
    octets[taille..2 * taille].fill(13); // +3 partout
    octets[2 * taille] = 255; // un seul point change de 242
    let m = analyse::mouvement_image(&octets);
    assert_eq!(m, vec![3.0, noyau::arrondi::arrondi((3.0 * (taille - 1) as f64 + 242.0) / taille as f64, 3)]);
    assert!(analyse::mouvement_image(&octets[..taille]).is_empty());
}
