//! Les mesures d'Inter contre `QFontMetricsF` du Monteur 1 (`outils/reference_composeur.py`) : les
//! étiquettes se taillent sur elles.

use composeur::police::Polices;
use serde_json::Value;

#[test]
fn mesures_comme_qt() {
    let r: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/donnees/reference.json")).unwrap()).unwrap();
    let p = Polices::charger();
    let mut pire = 0.0f32;
    for m in r["polices"].as_array().unwrap() {
        let (gras, px) = (m["gras"].as_bool().unwrap(), m["px"].as_f64().unwrap() as f32);
        let police = p.de(gras);
        assert_eq!(police.ascent(px), m["ascent"].as_f64().unwrap() as f32, "ascendante {px}");
        assert_eq!(police.descent(px), m["descent"].as_f64().unwrap() as f32, "descendante {px}");
        assert_eq!(police.hauteur(px), m["hauteur"].as_f64().unwrap() as f32, "hauteur {px}");
        for (texte, attendu) in m["avances"].as_object().unwrap() {
            let ecart = (police.avance(texte, px) - attendu.as_f64().unwrap() as f32).abs();
            println!("{gras} {px} « {texte} » : écart {ecart:.3} px");
            pire = pire.max(ecart);
        }
    }
    println!("pire écart d'avance : {pire:.3} px");
    assert!(pire < 0.5, "pire écart d'avance {pire} px");
}
