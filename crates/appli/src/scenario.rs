//! Un déroulé d'essai, pour essayer la fenêtre sans souris (sous Wayland, rien ne la pilote du
//! dehors) : `MONTEUR2_SCENARIO="ouvrir 0; aller 26.7; attendre 1; capture /tmp/x.png; quitter"`.
//! Étapes : `ouvrir <rang>`, `aller <s>`, `lire`, `arreter`, `attendre <s>`, `choisir zoom|etiquette|
//! coupe <rang>`, `preparer` (attend la fin), `couper <a> <b>`, `defaire`, `capture <fichier.png>`
//! (l'image de la visionneuse), `dire` (la position et l'état sur la sortie), `quitter`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::Monteur;
use crate::etat::{Choix, Sorte};

pub struct Scenario {
    etapes: VecDeque<String>,
    jusqua: Option<Instant>,
}

impl Scenario {
    pub fn depuis_l_environnement() -> Option<Self> {
        let texte = std::env::var("MONTEUR2_SCENARIO").ok()?;
        Some(Self { etapes: texte.split(';').map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect(), jusqua: None })
    }

    /// Fait les étapes qui peuvent l'être à ce tour ; `true` : quitter.
    pub fn avancer(&mut self, m: &mut Monteur) -> bool {
        if self.jusqua.is_some_and(|j| Instant::now() < j) {
            return false;
        }
        self.jusqua = None;
        // Une préparation en cours se laisse finir.
        if m.prise.as_ref().is_some_and(|p| p.preparation.is_some()) {
            return false;
        }
        while let Some(etape) = self.etapes.pop_front() {
            let mots: Vec<&str> = etape.split_whitespace().collect();
            let nombre = |i: usize| mots.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0);
            println!("scénario : {etape}");
            match mots[0] {
                "ouvrir" => {
                    if let Some((p, _)) = m.dossier.prises.get(nombre(1) as usize) {
                        m.a_ouvrir = Some(p.clone());
                    }
                    self.jusqua = Some(Instant::now() + Duration::from_millis(300));
                    return false;
                }
                "aller" => m.prise.iter_mut().for_each(|p| p.lecteur.aller_a(nombre(1))),
                "lire" => m.prise.iter_mut().for_each(|p| p.lecteur.lire()),
                "arreter" => m.prise.iter_mut().for_each(|p| p.lecteur.arreter()),
                "attendre" => {
                    self.jusqua = Some(Instant::now() + Duration::from_secs_f64(nombre(1)));
                    return false;
                }
                "choisir" => {
                    let sorte = match mots.get(1).copied() {
                        Some("zoom") => Sorte::Zoom,
                        Some("etiquette") => Sorte::Etiquette,
                        Some("coupe") => Sorte::Coupe,
                        _ => Sorte::SousTitre,
                    };
                    m.prise.iter_mut().for_each(|p| p.choix = Some(Choix { sorte, indice: nombre(2) as usize }));
                }
                "preparer" => {
                    m.prise.iter_mut().for_each(|p| p.preparer());
                    return false;
                }
                "couper" => m.prise.iter_mut().for_each(|p| p.ajouter_coupe(nombre(1), nombre(2))),
                "defaire" => m.prise.iter_mut().for_each(|p| p.defaire()),
                "capture" => {
                    if let Some(f) = mots.get(1) {
                        m.vue.enregistrer(f);
                    }
                }
                "dire" => {
                    if let Some(p) = &m.prise {
                        let mo = &p.montage;
                        println!(
                            "  position {:.3} s, en lecture {}, {} zooms, {} étiquettes, {} clics, {} coupes, durée montée {:.1} s ; {}",
                            p.lecteur.position(),
                            p.lecteur.en_lecture(),
                            mo.zooms.len(),
                            mo.etiquettes.len(),
                            mo.clics.len(),
                            mo.coupes.len(),
                            mo.duree_montee(),
                            m.message
                        );
                    }
                }
                "quitter" => return true,
                autre => eprintln!("scénario : étape inconnue « {autre} »"),
            }
        }
        false
    }
}
