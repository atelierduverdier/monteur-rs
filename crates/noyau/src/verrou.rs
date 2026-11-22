//! Un seul Monteur à la fois, le 1 ou le 2 : deux fenêtres réécriraient chacune les mêmes fichiers
//! depuis leur mémoire, et l'une effacerait ce que l'autre a noté. Le verrou est celui du Monteur 1
//! (`QLockFile`, `~/.local/share/monteur/monteur.verrou`), au même format : numéro du processus,
//! nom du programme, machine, identifiant de la machine, identifiant du démarrage. Qt juge un verrou
//! abandonné quand son processus est mort ou ne porte pas ce nom : le Monteur 1 respecte donc celui
//! du Monteur 2, et inversement.
//!
//! Par-dessus, le Monteur 2 garde le fichier verrouillé par le système (`File::try_lock`) tant
//! qu'il est ouvert. Sous Windows, où `/proc` n'existe pas, c'est ce qui dit qu'il tourne ; et un
//! Monteur qui plante lâche ce verrou-là tout seul.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Erreur;

pub struct Verrou {
    chemin: PathBuf,
    contenu: String,
    /// Le fichier ouvert et verrouillé : lâché avant de l'effacer.
    fichier: Option<std::fs::File>,
}

fn lire_ligne(chemin: &str) -> String {
    std::fs::read_to_string(chemin).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Le nom du programme d'un processus vivant, `None` s'il est mort.
#[cfg(target_os = "linux")]
fn processus(pid: u32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    let vivant = Path::new(&format!("/proc/{pid}")).exists();
    vivant.then(|| exe.and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default())
}

/// Ailleurs que sous Linux, le numéro ne dit rien : seul le verrou du système compte.
#[cfg(not(target_os = "linux"))]
fn processus(_pid: u32) -> Option<String> {
    None
}

/// Un autre programme ouvert (un autre Monteur 2) tient le fichier verrouillé.
fn verrouille_ailleurs(chemin: &Path) -> bool {
    let Ok(f) = std::fs::File::open(chemin) else { return false };
    match f.try_lock() {
        Ok(()) => {
            let _ = f.unlock();
            false
        }
        Err(std::fs::TryLockError::WouldBlock) => true,
        Err(_) => false,
    }
}

impl Verrou {
    /// Prend le verrou ; une erreur en français si un Monteur est déjà ouvert. Un verrou abandonné
    /// (processus mort, ou un autre programme sous ce numéro) est repris.
    pub fn prendre(chemin: &Path) -> Result<Self, Erreur> {
        if let Some(d) = chemin.parent() {
            std::fs::create_dir_all(d).map_err(|e| Erreur(format!("{} : {e}", d.display())))?;
        }
        if let Ok(ancien) = std::fs::read_to_string(chemin) {
            let mut lignes = ancien.lines();
            let pid = lignes.next().and_then(|l| l.trim().parse::<u32>().ok());
            let nom = lignes.next().unwrap_or("").trim().to_string();
            let tenu = pid.and_then(processus).is_some_and(|n| n.is_empty() || nom.is_empty() || n == nom) || verrouille_ailleurs(chemin);
            if tenu && pid != Some(std::process::id()) {
                return Err(Erreur("Le Monteur est déjà ouvert.".into()));
            }
            let _ = std::fs::remove_file(chemin);
        }
        let nom = std::env::current_exe()
            .ok()
            .and_then(|e| e.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "monteur2".into());
        let contenu = format!(
            "{}\n{nom}\n{}\n{}\n{}\n",
            std::process::id(),
            lire_ligne("/proc/sys/kernel/hostname"),
            lire_ligne("/etc/machine-id"),
            lire_ligne("/proc/sys/kernel/random/boot_id")
        );
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(chemin)
            .map_err(|_| Erreur("Le Monteur est déjà ouvert.".into()))?;
        f.write_all(contenu.as_bytes()).map_err(|e| Erreur(format!("{} : {e}", chemin.display())))?;
        // Refusé (un système de fichiers sans verrous) : le contenu suffit sous Linux.
        let _ = f.try_lock();
        Ok(Self { chemin: chemin.to_path_buf(), contenu, fichier: Some(f) })
    }
}

impl Drop for Verrou {
    fn drop(&mut self) {
        // Lâché d'abord : sous Windows, un fichier verrouillé ne se relit pas.
        drop(self.fichier.take());
        // Seulement s'il est toujours le nôtre.
        if std::fs::read_to_string(&self.chemin).is_ok_and(|c| c == self.contenu) {
            let _ = std::fs::remove_file(&self.chemin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jetable(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("monteur-verrou-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("monteur.verrou")
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pris_rendu_et_abandonne() {
        let chemin = jetable("cycle");
        {
            let _v = Verrou::prendre(&chemin).unwrap();
            let texte = std::fs::read_to_string(&chemin).unwrap();
            assert_eq!(texte.lines().count(), 5, "le format de QLockFile");
            assert_eq!(texte.lines().next().unwrap(), std::process::id().to_string());
        }
        assert!(!chemin.exists(), "rendu à la fermeture");
        // Un verrou d'un processus mort se reprend.
        std::fs::write(&chemin, "999999999\npython3.14\natomman\n").unwrap();
        drop(Verrou::prendre(&chemin).unwrap());
        // Tenu par un processus vivant du bon nom : refusé.
        let mut enfant = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        std::fs::write(&chemin, format!("{}\nsleep\natomman\n", enfant.id())).unwrap();
        let e = Verrou::prendre(&chemin).err().expect("déjà tenu");
        assert!(e.0.contains("déjà ouvert"));
        // Le même numéro sous un autre nom de programme : abandonné, repris.
        std::fs::write(&chemin, format!("{}\npython3.14\natomman\n", enfant.id())).unwrap();
        assert!(Verrou::prendre(&chemin).is_ok());
        let _ = enfant.kill();
        let _ = enfant.wait();
        let _ = std::fs::remove_dir_all(chemin.parent().unwrap());
    }
}

#[cfg(test)]
mod tests_systeme {
    use super::*;

    /// Un autre Monteur 2 tient le fichier : refusé, quel que soit le numéro écrit dedans (sous
    /// Windows, il ne dit rien). Lâché, parce qu'il s'est fermé ou a planté : repris.
    #[test]
    fn verrouille_par_un_autre_monteur() {
        let d = std::env::temp_dir().join(format!("monteur-verrou-systeme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let chemin = d.join("monteur.verrou");
        std::fs::write(&chemin, "999999999\nmonteur2\n").unwrap();
        let autre = std::fs::File::open(&chemin).unwrap();
        autre.try_lock().unwrap();
        let e = Verrou::prendre(&chemin).err().expect("tenu par le système");
        assert!(e.0.contains("déjà ouvert"));
        drop(autre);
        let v = Verrou::prendre(&chemin).unwrap();
        assert!(verrouille_ailleurs(&chemin), "le Monteur ouvert tient son fichier");
        drop(v);
        assert!(!chemin.exists(), "rendu à la fermeture");
        let _ = std::fs::remove_dir_all(&d);
    }
}
