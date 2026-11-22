//! Un déroulé d'essai, pour essayer la fenêtre sans souris (sous Wayland, rien ne la pilote du
//! dehors) : `MONTEUR2_SCENARIO="ouvrir 0; aller 26.7; attendre 1; capture /tmp/x.png; quitter"`.
//! Étapes : `ouvrir <rang>`, `aller <s>`, `lire`, `arreter`, `attendre <s>`, `choisir zoom|etiquette|
//! coupe <rang>`, `preparer` (attend la fin), `zoom <s>`, `tracer <ax> <ay> <bx> <by>`, `stop`, `projet <nom>`, `inserer dessus|intercalee <t> <fichier>`, `titre <texte>`,
//! `nouveau <nom>`, `enregistrer`, `chapitre <s>`, `proposer_chapitres`, `scinder <s>`,
//! `accelerer <a> <b>`, `inserer_chapitres`, `proposer`, `envoyer` (faux programme seulement),
//! `refaire_sous_titres`, `rendre` (attend la fin), `rendre_en_fond`, `arreter_rendu`, `filmer`, `filmer_ecran`, `filmer_simple`, `arreter_ecran`, `niveau`, `fermer_projet`, `onglet_vignette`, `fenetre_voix`, `fenetre_rendre`, `file <préréglage>`, `demander_corbeille <rang>`, `confirmer_corbeille`, `onglet_texte`, `couper_phrase <rang>`, `liste`, `marquer` (touche C), `ecrire_vignette`, `capture_vignette <f.png>`, `couper <a> <b>`, `defaire`, `capture <fichier.png>`
//! (l'image de la visionneuse), `dire` (la position et l'état sur la sortie), `dire_voix` (les réglages
//! de la voix), `midi <cc> <valeur>` (un message du nanoKONTROL2 : 127 appuie un bouton, 0 le relâche),
//! `planter` (essai du journal des plantages), `quitter`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::Monteur;
use crate::etat::{Choix, Sorte};

pub struct Scenario {
    etapes: VecDeque<String>,
    jusqua: Option<Instant>,
    /// `rendre_en_fond` : les étapes suivantes n'attendent pas la fin du rendu.
    sans_attendre: bool,
}

impl Scenario {
    pub fn depuis_l_environnement() -> Option<Self> {
        let texte = std::env::var("MONTEUR2_SCENARIO").ok()?;
        Some(Self { etapes: texte.split(';').map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect(), jusqua: None, sans_attendre: false })
    }

    /// Fait les étapes qui peuvent l'être à ce tour ; `true` : quitter.
    pub fn avancer(&mut self, m: &mut Monteur) -> bool {
        if self.jusqua.is_some_and(|j| Instant::now() < j) {
            return false;
        }
        self.jusqua = None;
        // Une préparation en cours se laisse finir.
        if m.prise.as_ref().is_some_and(|p| p.preparation.is_some()) || (m.rendu.is_some() && !self.sans_attendre) {
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
                "zoom" => m.prise.iter_mut().for_each(|p| p.nouveau_zoom(nombre(1))),
                "tracer" => m.prise.iter_mut().for_each(|p| {
                    if let Some(Choix { sorte: Sorte::Zoom, indice }) = p.choix {
                        p.retenir();
                        let z = &mut p.montage.zooms[indice];
                        (z.facteur, z.cx, z.cy) = crate::etat::zoom_trace(nombre(1), nombre(2), nombre(3), nombre(4));
                        p.modifie();
                    }
                }),
                "projet" => m.ranger_dans_un_projet(&mots[1..].join(" ")),
                // `inserer <fichier|dessus|intercalee> …` : le dialogue de fichiers ne se pilote pas.
                "inserer" => {
                    let par_dessus = mots.get(1) == Some(&"dessus");
                    let source = std::path::PathBuf::from(mots[3..].join(" "));
                    if let Some(p) = &mut m.prise {
                        match crate::importer(&source, &p.dossier().to_path_buf(), par_dessus, nombre(2)) {
                            Ok((v, d, b, t)) => p.ajouter_insertion(v, d, b, t),
                            Err(e) => eprintln!("{e}"),
                        }
                    }
                }
                "titre" => {
                    if let Some(p) = &m.prise {
                        m.fiches.fiches.entry(noyau::videos::cle(&p.video)).or_default().titre = mots[1..].join(" ");
                        m.vignette.salir();
                    }
                }
                "ecrire_vignette" => {
                    if let Some(p) = &m.prise {
                        let f = m.fiches.fiches.entry(noyau::videos::cle(&p.video)).or_default();
                        println!("  {}", m.vignette.ecrire_pour_essai(f, &p.video));
                    }
                }
                "capture_vignette" => {
                    if let Some(f) = mots.get(1) {
                        m.vignette.enregistrer(f);
                    }
                }
                "enregistrer" => m.enregistrer(),
                // Jamais d'envoi réel depuis un essai : il faut le faux programme.
                "envoyer" => {
                    if std::env::var_os("MONTEUR_YOUTUBE_ENVOI").is_some() {
                        m.envoyer();
                        self.jusqua = Some(Instant::now() + Duration::from_secs(2));
                        return false;
                    }
                    eprintln!("scénario : envoi refusé sans MONTEUR_YOUTUBE_ENVOI");
                }
                "chapitre" => m.prise.iter_mut().for_each(|p| {
                    p.poser_chapitre(nombre(1));
                }),
                "proposer_chapitres" => {
                    let gestes = crate::chemin_fiches().with_file_name("gestes");
                    if let Some(p) = &mut m.prise {
                        let d = [p.dossier().to_path_buf(), gestes];
                        println!("  {}", p.proposer_chapitres(&d));
                    }
                }
                "scinder" => m.prise.iter_mut().for_each(|p| println!("  {}", p.scinder(nombre(1)))),
                "accelerer" => m.prise.iter_mut().for_each(|p| {
                    p.retenir();
                    p.montage.coupes.push(noyau::montage::Coupe { debut: nombre(1), fin: nombre(2), active: true, origine: "main".into(), vitesse: 4.0, autres: Default::default() });
                    p.montage.trier();
                    p.modifie();
                }),
                "inserer_chapitres" => {
                    if let Some(p) = &m.prise {
                        crate::youtube::inserer_chapitres(&mut m.youtube.description, &p.video, &p.montage);
                        println!("  {}", m.youtube.description.texte.replace('\n', " | "));
                    }
                }
                "demander_corbeille" => {
                    if let Some((p, _)) = m.dossier.prises.get(nombre(1) as usize) {
                        let p = p.clone();
                        m.demander_corbeille(p);
                    }
                }
                "confirmer_corbeille" => {
                    if let Some(v) = m.a_jeter.take() {
                        m.mettre_a_la_corbeille(&v);
                    }
                }
                "fenetre_voix" => m.voix.ouvrir(),
                "ce_qu_il_faut" => m.ouvrir_ce_qu_il_faut = true,
                "a_propos" => m.ouvrir_a_propos = true,
                "fenetre_rendre" => m.fenetre_rendre = true,
                "file" => m.mettre_en_file(nombre(1) as usize, false),
                "fermer_projet" => m.fermer_projet(),
                "onglet_vignette" => m.montrer_vignette = true,
                "onglet_texte" => m.montrer_texte = true,
                "onglet_youtube" => m.montrer_youtube = true,
                "proposer" => {
                    if let Some(p) = &m.prise {
                        let f = m.fiches.fiches.entry(noyau::videos::cle(&p.video)).or_default();
                        m.youtube.description = noyau::description::description_proposee(f);
                    }
                }
                "refaire_sous_titres" => {
                    m.prise.iter_mut().for_each(|p| p.refaire_les_sous_titres());
                    return false;
                }
                "rendre_en_fond" => {
                    self.sans_attendre = true;
                    m.rendre();
                }
                "arreter_rendu" => {
                    self.sans_attendre = false;
                    if let Some(r) = &m.rendu {
                        r.suivi.arret.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                "rendre" => {
                    m.rendre();
                    return false;
                }
                "filmer" => m.filmer(),
                "filmer_ecran" => m.filmer_ecran(),
                "filmer_simple" => m.filmer_simple(),
                "couper_phrase" => m.prise.iter_mut().for_each(|p| println!("  {}", p.basculer_phrase(nombre(1) as usize))),
                "niveau" => {
                    if let Some(c) = &m.capture {
                        println!("  micro {:?} dB, muet {}, temps {:?}, décompte {:?}", c.niveau(), c.micro_muet(), c.temps(), c.decompte_restant());
                    }
                }
                "arreter_ecran" => m.capture.iter_mut().for_each(|c| c.arreter()),
                "liste" => println!("  {} prise(s) dans la liste", m.dossier.prises.len()),
                "nouveau" => m.nouveau_projet(&mots[1..].join(" ")),
                "marquer" => m.prise.iter_mut().for_each(|p| {
                    let t = p.lecteur.position();
                    let _ = p.marquer_coupe(t);
                }),
                "stop" => m.prise.iter_mut().for_each(|p| p.lecteur.stop()),
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
                        let zoom = match p.choix {
                            Some(Choix { sorte: Sorte::Zoom, indice }) => {
                                mo.zooms.get(indice).map(|z| format!(", zoom choisi ×{} ({}, {})", z.facteur, z.cx, z.cy)).unwrap_or_default()
                            }
                            _ => String::new(),
                        };
                        println!(
                            "  position {:.3} s, en lecture {}, {} zooms, {} étiquettes, {} clics, {} coupes, {} chapitres, durée montée {:.1} s{zoom} ; {}",
                            p.lecteur.position(),
                            p.lecteur.en_lecture(),
                            mo.zooms.len(),
                            mo.etiquettes.len(),
                            mo.clics.len(),
                            mo.coupes.len(),
                            mo.chapitres.len(),
                            mo.duree_montee(),
                            m.message
                        );
                    }
                }
                "dire_voix" => println!("  voix {:?}, fenêtre ouverte {}", m.voix.reglages, m.voix.ouverte),
                "midi" => m.midi_cc(nombre(1) as u8, nombre(2) as u8),
                "quitter" => return true,
                // Pour vérifier que le journal des plantages s'écrit.
                "planter" => panic!("plantage voulu par le déroulé d'essai"),
                autre => eprintln!("scénario : étape inconnue « {autre} »"),
            }
        }
        false
    }
}
