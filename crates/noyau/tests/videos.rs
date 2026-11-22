//! La liste d'un dossier, l'état des vidéos, le rangement, ffprobe, le cadre de la vignette et
//! l'envoi, contre le Monteur 1 (`outils/reference_python.py`). Le dossier de vidéos est un dossier
//! jetable bâti comme celui du Monteur 1 ; aucun envoi : le programme d'envoi est un faux.

use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use noyau::description::Description;
use noyau::fiches::Fiches;
use noyau::videos;
use serde_json::Value;

fn reference() -> Value {
    let chemin = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/donnees/reference.json");
    serde_json::from_str(&std::fs::read_to_string(chemin).unwrap()).unwrap()
}

fn jetable(nom: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("monteur-videos-{nom}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn noms(chemins: &[PathBuf]) -> Vec<String> {
    chemins.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

fn contenu(d: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

fn json<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap()
}

#[test]
fn liste_etats_et_rangement_comme_le_monteur1() {
    let r = reference();
    let s = &r["videos"];
    let tmp = jetable("rangement");
    let (dossier, rangement) = (tmp.join("Copies d'écran"), tmp.join("YouTube"));
    std::fs::create_dir_all(&dossier).unwrap();
    let maintenant = s["maintenant"].as_u64().unwrap();
    for ligne in s["arbre"].as_array().unwrap() {
        let (nom, age) = (ligne[0].as_str().unwrap(), ligne[1].as_u64().unwrap());
        std::fs::write(dossier.join(nom), nom).unwrap();
        let f = std::fs::File::options().write(true).open(dossier.join(nom)).unwrap();
        f.set_modified(UNIX_EPOCH + Duration::from_secs(maintenant - age)).unwrap();
    }
    let chemin_fiches = tmp.join("fiches.json");
    let mut fiches = Fiches::default();
    let fiche = |fiches: &mut Fiches, nom: &str| fiches.fiches.entry(videos::cle(&dossier.join(nom))).or_default().clone();
    let mut f = fiche(&mut fiches, "2026-11-01 07-30-28 avec vignette.mp4");
    f.youtube_id = "kO3T6NuRUxk".into();
    fiches.fiches.insert(videos::cle(&dossier.join("2026-11-01 07-30-28 avec vignette.mp4")), f);
    let mut f = fiche(&mut fiches, "bug ignoré.mp4");
    f.ignoree = true;
    fiches.fiches.insert(videos::cle(&dossier.join("bug ignoré.mp4")), f);
    let mut f = fiche(&mut fiches, "2026-11-04 08-12-40.mp4");
    f.titre = "GRAVER UNE IMAGE".into();
    fiches.fiches.insert(videos::cle(&dossier.join("2026-11-04 08-12-40.mp4")), f);

    let liste = videos::lister_videos(&dossier);
    assert_eq!(json(noms(&liste)), s["liste"], "liste");
    for p in &liste {
        let nom = p.file_name().unwrap().to_string_lossy().into_owned();
        let etat = videos::etat(p, fiches.fiches.get(&videos::cle(p)));
        assert_eq!(etat, s["etats"][&nom], "état de {nom}");
        assert_eq!(json(noms(&videos::fichiers_lies(p))), s["lies"][&nom], "fichiers liés à {nom}");
        assert_eq!(json(videos::est_derivee(p)), s["derivees"][&nom]);
        assert_eq!(json(videos::nom_source(p)), s["sources"][&nom]);
    }

    let nouvelle = videos::deplacer(&mut fiches, &chemin_fiches, &dossier.join("2026-11-04 08-12-40.mp4"), &rangement).unwrap();
    assert_eq!(nouvelle.strip_prefix(videos::cle(&tmp)).unwrap().to_str(), s["range"].as_str());
    assert_eq!(json(contenu(&dossier)), s["apres"]["dossier"]);
    assert_eq!(json(contenu(&rangement)), s["apres"]["rangement"]);
    let relues = Fiches::lire(&chemin_fiches).unwrap();
    let mut cles: Vec<String> =
        relues.fiches.keys().map(|k| Path::new(k).strip_prefix(videos::cle(&tmp)).unwrap().to_string_lossy().into_owned()).collect();
    cles.sort();
    assert_eq!(json(cles), s["fiches_apres"], "la fiche a suivi la vidéo");
    let tmp_cle = videos::cle(&tmp);

    // Rien n'est écrasé : un fichier lié déjà là-bas, et rien ne bouge.
    std::fs::write(dossier.join("2026-11-03 07-55-02 vignette YouTube.png"), "x").unwrap();
    std::fs::write(rangement.join("2026-11-03 07-55-02 vignette YouTube.png"), "y").unwrap();
    let e = videos::deplacer(&mut fiches, &chemin_fiches, &dossier.join("2026-11-03 07-55-02.mp4"), &rangement).unwrap_err();
    assert_eq!(e.0.replace(&tmp_cle, "<tmp>"), s["refus"].as_str().unwrap());
    assert_eq!(json(contenu(&dossier)), s["apres_refus"]);
    assert_eq!(std::fs::read_to_string(rangement.join("2026-11-03 07-55-02 vignette YouTube.png")).unwrap(), "y");
    std::fs::remove_dir_all(&tmp).unwrap();
}

#[test]
fn ffprobe_et_montage_comme_le_monteur1() {
    let r = reference();
    let sondes = r["sondes"].as_object().unwrap();
    assert!(sondes.contains_key("vraie"), "reference.json fait sans la vraie prise");
    for (nom, attendu) in sondes {
        let infos = videos::lire_infos(attendu["texte"].as_str().unwrap()).unwrap();
        assert_eq!(json(&infos), attendu["infos"], "infos {nom}");
        let cmd = videos::commande_montage(
            Path::new("/v/x %d.mp4"),
            Path::new("/t/vignette.png"),
            Path::new("/v/x avec vignette.partiel.mp4"),
            &infos,
            None,
        );
        assert_eq!(json(cmd), attendu["montage"], "montage {nom}");
        // Avec le carton : trois morceaux, le carton en queue, du silence sous lui s'il y a du son.
        let avec = videos::commande_montage(Path::new("/v/x.mp4"), Path::new("/t/v.png"), Path::new("/v/s.mp4"), &infos, Some(Path::new("/t/carton.png"))).join(" ");
        assert!(avec.contains("concat=n=3"), "{avec}");
        assert!(avec.contains("-t 2.0 -f image2 -pattern_type none -i /t/carton.png"), "{avec}");
        assert_eq!(avec.contains("[v2][4:a]"), infos.son, "{avec}");
        let av: Vec<_> = ["out_time_us=1500000", "out_time_us=999999999999", "frame=12", " out_time_us=42"]
            .iter()
            .map(|l| videos::avancement_montage(l, &infos))
            .collect();
        let attendu_av: Vec<Option<f64>> = serde_json::from_value(attendu["avancement"].clone()).unwrap();
        for (a, b) in av.iter().zip(&attendu_av) {
            assert!(a.zip(*b).is_none_or(|(a, b)| (a - b).abs() < 1e-12) && a.is_some() == b.is_some(), "avancement {nom}");
        }
    }
    assert!(videos::lire_infos(r#"{"streams": [{"codec_type": "audio"}]}"#).is_err());
}

#[test]
fn cadres_images_et_partiels_comme_le_monteur1() {
    let r = reference();
    for (taille, attendu) in r["cadres"].as_object().unwrap() {
        let (l, h) = taille.split_once('x').unwrap();
        let (l, h): (i64, i64) = (l.parse().unwrap(), h.parse().unwrap());
        assert_eq!(json(videos::cadre_defaut(l, h)), attendu["defaut"], "cadre par défaut {taille}");
        let essais = [[-50.0, -50.0, 100.0, 0.0], [1e5, 1e5, 1e5, 0.0], [10.5, 20.5, 400.5, 0.0], [300.0, 100.0, 646.0, 720.0], [0.0, 0.0, 159.4, 0.0]];
        let bornes: Vec<_> = essais.iter().map(|&c| videos::borner_cadre(c, l, h)).collect();
        assert_eq!(json(bornes), attendu["bornes"], "cadres bornés {taille}");
    }
    let images: Vec<_> = [(12.3456, None), (99.0, Some(10.0)), (-3.0, Some(5.0)), (4.0, Some(0.0))]
        .iter()
        .map(|&(t, d)| videos::commande_image(Path::new("/v/x.mp4"), t, d))
        .collect();
    assert_eq!(json(images), r["images"]);
    let partiels: Vec<String> = ["/v/x avec vignette.mp4", "/v/x.vignette.png", "/v/sans"]
        .iter()
        .map(|p| videos::chemin_partiel(Path::new(p)).to_string_lossy().into_owned())
        .collect();
    assert_eq!(json(partiels), r["partiels"]);
}

#[test]
fn youtube_comme_le_monteur1_sans_jamais_rien_envoyer() {
    let r = reference();
    // SAFETY du test : seul ce test lit cette variable, et elle désigne un faux programme.
    unsafe { std::env::set_var("MONTEUR_YOUTUBE_ENVOI", "/faux/youtube-envoi") };
    let descriptions = [
        Description::default(),
        Description { titre: "t".repeat(101), texte: "é".repeat(2501), mots_cles: vec!["mot"; 130].join(",") },
        Description { titre: "Graver <une> image".into(), texte: "a > b".into(), mots_cles: "VerdierCAM, CNC\nFAO".into() },
    ];
    let problemes: Vec<_> = descriptions.iter().map(videos::problemes_description).collect();
    assert_eq!(json(problemes), r["problemes"]);
    let vignettes = [None, Some(Path::new("/v/x vignette YouTube.jpg")), None];
    let envois: Vec<_> = descriptions
        .iter()
        .zip(vignettes)
        .map(|(d, v)| videos::commande_envoi(Path::new("/v/x avec vignette.mp4"), d, Path::new("/v/x description YouTube.txt"), v))
        .collect();
    assert_eq!(json(&envois), r["envois"]);
    assert!(envois.iter().all(|c| c[0] == "/faux/youtube-envoi" && c.windows(2).any(|w| w[0] == "--confidentialite" && w[1] == "privee")));
    let ids: Vec<_> = [
        "Envoyée : https://youtu.be/kO3T6NuRUxk (privée)",
        "https://youtu.be/abc",
        "rien",
        "https://youtu.be/a_b-c9Zé!",
    ]
    .iter()
    .map(|s| videos::lire_id_youtube(s))
    .collect();
    assert_eq!(json(ids), r["ids"]);
    let pourcents: Vec<_> = ["Envoi : 42,5 %", "12%", "x 7.25  % y", "100 %", "aucun", "3 4 %"].iter().map(|s| videos::lire_pourcentage(s)).collect();
    assert_eq!(json(pourcents), r["pourcentages"]);
}
