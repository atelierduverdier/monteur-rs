//! Télécharger les modèles de Whisper, à la demande : curl, celui du système (Windows 10 et 11 l'ont, Linux presque
//! toujours), dans un fichier `.partiel`, l'empreinte SHA-256 vérifiée avant de poser le fichier sous son nom. Un
//! fichier déjà là, à la bonne taille et à la bonne empreinte, ne se retélécharge pas.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use noyau::sous_titres::ATelecharger;
use sha2::{Digest, Sha256};

/// L'empreinte SHA-256 d'un fichier, en hexadécimal.
pub fn empreinte(chemin: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(chemin)?;
    let mut h = Sha256::new();
    let mut tampon = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut tampon)?;
        if n == 0 {
            break;
        }
        h.update(&tampon[..n]);
    }
    Ok(h.finalize().iter().map(|o| format!("{o:02x}")).collect())
}

fn deja_la(cible: &Path, f: &ATelecharger) -> bool {
    std::fs::metadata(cible).is_ok_and(|m| m.len() == f.taille) && empreinte(cible).is_ok_and(|e| e == f.sha256)
}

/// Le morceau déjà reçu d'un fichier, à reprendre : sa taille, 0 s'il n'y en a pas.
fn deja_recu(partiel: &Path) -> u64 {
    std::fs::metadata(partiel).map_or(0, |m| m.len())
}

/// Un fichier : curl jusqu'au bout (ou l'arrêt), puis l'empreinte.
///
/// LA REPRISE (23/11/2026, Christophe : « si je ferme le Monteur, ça va couper le téléchargement », puis « fais-le ») :
/// arrêté, ou coupé par le réseau, le `.partiel` reste ; le téléchargement suivant repart de là (`--continue-at`).
/// Il n'est effacé que s'il ne peut pas servir : une empreinte fausse au bout, un serveur qui refuse la reprise ou
/// répond par une erreur, un morceau plus gros que le fichier.
fn un_fichier(f: &ATelecharger, dossier: &Path, arret: &AtomicBool, recus: &AtomicU64, avant: u64) -> Result<(), String> {
    let cible = dossier.join(f.nom);
    if deja_la(&cible, f) {
        recus.store(avant + f.taille, Ordering::Relaxed);
        return Ok(());
    }
    let partiel = dossier.join(format!("{}.partiel", f.nom));
    if deja_recu(&partiel) > f.taille {
        let _ = std::fs::remove_file(&partiel);
    }
    if deja_recu(&partiel) < f.taille {
        recevoir(f, &partiel, arret, recus, avant)?;
    }
    let e = empreinte(&partiel).map_err(|e| e.to_string())?;
    if e != f.sha256 {
        let _ = std::fs::remove_file(&partiel);
        return Err(format!("{} : empreinte inattendue, fichier écarté", f.nom));
    }
    std::fs::rename(&partiel, &cible).map_err(|e| format!("{} : {e}", cible.display()))
}

/// curl, de là où le `.partiel` s'est arrêté jusqu'au bout du fichier.
fn recevoir(f: &ATelecharger, partiel: &Path, arret: &AtomicBool, recus: &AtomicU64, avant: u64) -> Result<(), String> {
    let mut commande = noyau::systeme::commande("curl");
    commande.args(["--location", "--fail", "--silent", "--show-error", "--retry", "3"]);
    if deja_recu(partiel) > 0 {
        commande.args(["--continue-at", "-"]);
    }
    let mut curl = commande
        .arg("--output")
        .arg(partiel)
        .arg(f.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl n'a pas pu être lancé ({e}) : il faut curl pour télécharger"))?;
    let etat = loop {
        if arret.load(Ordering::Relaxed) {
            let _ = curl.kill();
            let _ = curl.wait();
            return Err("Téléchargement arrêté : il reprendra où il en était.".into());
        }
        recus.store(avant + std::fs::metadata(&partiel).map_or(0, |m| m.len()), Ordering::Relaxed);
        match curl.try_wait() {
            Ok(Some(e)) => break e,
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    };
    recus.store(avant + deja_recu(partiel), Ordering::Relaxed);
    if !etat.success() {
        let mut erreur = String::new();
        if let Some(mut e) = curl.stderr.take() {
            let _ = e.read_to_string(&mut erreur);
        }
        // 22 : le serveur répond par une erreur (dont « plage impossible ») ; 33 : il ne sait pas reprendre. Le
        // morceau ne servirait pas. Le réseau coupé (6, 7, 18, 28, 56…) le laisse, pour la reprise.
        if matches!(etat.code(), Some(22 | 33)) || deja_recu(partiel) == 0 {
            let _ = std::fs::remove_file(partiel);
        }
        return Err(format!("{} : {}", f.nom, erreur.lines().last().unwrap_or("curl a échoué").trim()));
    }
    Ok(())
}

/// Des fichiers téléchargés l'un après l'autre, dans un fil.
pub struct Telechargement {
    total: u64,
    recus: Arc<AtomicU64>,
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<Result<(), String>>>,
}

impl Telechargement {
    pub fn lancer(fichiers: &'static [ATelecharger], dossier: PathBuf) -> Self {
        let total = fichiers.iter().map(|f| f.taille).sum();
        let (recus, arret) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(false)));
        let (r, a) = (recus.clone(), arret.clone());
        let fil = std::thread::spawn(move || {
            std::fs::create_dir_all(&dossier).map_err(|e| format!("{} : {e}", dossier.display()))?;
            let mut avant = 0;
            for f in fichiers {
                un_fichier(f, &dossier, &a, &r, avant)?;
                avant += f.taille;
            }
            Ok(())
        });
        Self { total, recus, arret, fil: Some(fil) }
    }

    /// De 0 à 1.
    pub fn avance(&self) -> f64 {
        (self.recus.load(Ordering::Relaxed) as f64 / self.total.max(1) as f64).min(1.0)
    }

    pub fn arreter(&self) {
        self.arret.store(true, Ordering::Relaxed);
    }

    /// `Some` une fois, à la fin.
    pub fn suivre(&mut self) -> Option<Result<(), String>> {
        if !self.fil.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        Some(self.fil.take()?.join().unwrap_or_else(|_| Err("le téléchargement s'est arrêté".into())))
    }
}

impl Drop for Telechargement {
    fn drop(&mut self) {
        self.arreter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empreinte_connue() {
        let d = std::env::temp_dir().join(format!("monteur-empreinte-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("abc"), "abc").unwrap();
        let e = empreinte(&d.join("abc")).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(e, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Le vrai téléchargement du petit modèle de détection de voix (0,9 Mo), empreinte comprise ; puis, déjà là, il
    /// ne repart pas. `cargo test -p appli --release -- --ignored le_vad`.
    #[test]
    #[ignore = "télécharge 0,9 Mo depuis huggingface.co"]
    fn le_vad_pour_de_bon() {
        let d = std::env::temp_dir().join(format!("monteur-vad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let attendre = |t: &mut Telechargement| loop {
            if let Some(f) = t.suivre() {
                break f;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let vad = &noyau::sous_titres::A_TELECHARGER[1..];
        let premier = attendre(&mut Telechargement::lancer(vad, d.clone()));
        let date = std::fs::metadata(d.join(noyau::sous_titres::VAD)).and_then(|m| m.modified()).ok();
        let second = attendre(&mut Telechargement::lancer(vad, d.clone()));
        let date2 = std::fs::metadata(d.join(noyau::sous_titres::VAD)).and_then(|m| m.modified()).ok();
        let fichiers: Vec<_> = std::fs::read_dir(&d).unwrap().filter_map(Result::ok).map(|f| f.file_name()).collect();
        let _ = std::fs::remove_dir_all(&d);
        assert!(premier.is_ok(), "{premier:?}");
        assert!(second.is_ok() && date.is_some() && date == date2, "déjà là : pas retéléchargé");
        assert_eq!(fichiers.len(), 1, "{fichiers:?}");
    }

    /// Un petit serveur HTTP qui sait reprendre (`Range: bytes=N-`) : il sert `donnees` et note d'où part chaque
    /// demande.
    fn serveur(donnees: Vec<u8>) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<u64>>>) {
        use std::io::{BufRead, BufReader, Write};
        let ecoute = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = ecoute.local_addr().unwrap().port();
        let departs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let d = departs.clone();
        std::thread::spawn(move || {
            for flux in ecoute.incoming().flatten() {
                let mut lecteur = BufReader::new(flux.try_clone().unwrap());
                let mut depart = 0u64;
                loop {
                    let mut ligne = String::new();
                    if lecteur.read_line(&mut ligne).unwrap_or(0) == 0 || ligne == "\r\n" {
                        break;
                    }
                    let bas = ligne.to_ascii_lowercase();
                    if let Some(reste) = bas.strip_prefix("range: bytes=") {
                        depart = reste.trim().trim_end_matches('-').parse().unwrap_or(0);
                    }
                }
                d.lock().unwrap().push(depart);
                let corps = &donnees[depart as usize..];
                let entete = if depart > 0 {
                    format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {depart}-{}/{}\r\nConnection: close\r\n\r\n",
                        corps.len(), donnees.len() - 1, donnees.len())
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", corps.len())
                };
                let mut flux = flux;
                let _ = flux.write_all(entete.as_bytes());
                let _ = flux.write_all(corps);
            }
        });
        (port, departs)
    }

    /// LA REPRISE : un `.partiel` de 100 000 octets déjà là, le téléchargement demande la suite (et seulement la
    /// suite), et le fichier entier, à la bonne empreinte, prend son nom.
    #[test]
    fn reprend_ou_il_en_etait() {
        let donnees: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        let sha = format!("{:x}", Sha256::digest(&donnees));
        let (port, departs) = serveur(donnees.clone());
        let entree: &'static [ATelecharger] = Box::leak(Box::new([ATelecharger {
            nom: "essai.bin",
            url: format!("http://127.0.0.1:{port}/essai.bin").leak(),
            taille: donnees.len() as u64,
            sha256: sha.leak(),
        }]));
        let d = std::env::temp_dir().join(format!("monteur-reprise-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("essai.bin.partiel"), &donnees[..100_000]).unwrap();
        let mut t = Telechargement::lancer(entree, d.clone());
        let mut fin = None;
        for _ in 0..400 {
            fin = t.suivre();
            if fin.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let recu = std::fs::read(d.join("essai.bin")).unwrap_or_default();
        let partiel_reste = d.join("essai.bin.partiel").exists();
        let _ = std::fs::remove_dir_all(&d);
        assert!(matches!(fin, Some(Ok(()))), "{fin:?}");
        assert_eq!(*departs.lock().unwrap(), vec![100_000], "la suite seulement");
        assert!(recu == donnees && !partiel_reste);
    }

    /// Une adresse qui n'existe pas : une erreur claire, et pas de `.partiel` qui traîne.
    #[test]
    fn une_adresse_morte() {
        static MORT: [ATelecharger; 1] = [ATelecharger { nom: "rien.bin", url: "http://127.0.0.1:9/rien.bin", taille: 10, sha256: "0" }];
        let d = std::env::temp_dir().join(format!("monteur-telechargement-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let mut t = Telechargement::lancer(&MORT, d.clone());
        let mut fin = None;
        for _ in 0..200 {
            fin = t.suivre();
            if fin.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let restes = std::fs::read_dir(&d).map(|r| r.count()).unwrap_or(0);
        let _ = std::fs::remove_dir_all(&d);
        assert!(matches!(fin, Some(Err(e)) if e.contains("rien.bin")));
        assert_eq!(restes, 0);
    }
}
