# Portabilité, lot 1 : les briques communes à Linux et Windows (22/11/2026)

Pour donner le Monteur avec VerdierCAM, il doit tourner ailleurs que sur ce poste KDE. Ce premier
lot remplace ce qui ne marchait qu'ici par des briques qui marchent aussi sous Windows, sans rien
changer sous Linux : mêmes dossiers, même verrou que le Monteur 1, même dialogue de fichiers sous KDE.

| Avant | Maintenant | Où |
|---|---|---|
| `kdialog` | `rfd` : le portail sous Linux, le dialogue de Windows sous Windows | `choisir_un_fichier` (main.rs) |
| `gio trash` | `trash` : la corbeille du système, jamais de suppression définitive | `mettre_a_la_corbeille` |
| `xdg-open` | `opener` | `ouvrir_le_dossier` |
| `paplay` (bips, écoute de la voix), `parec` (vu-mètre) | cpal, comme la lecture | `media::sons` |
| `magick` (vignette en JPG) | `jpeg-encoder`, posé sur du blanc comme `-flatten`, qualité 92 en 4:4:4 puis 80 au-delà de 2 Mio, comme ImageMagick | `en_jpeg` (vignette.rs) |
| `~/Vidéos`, `~/.local/share/…`, `XDG_DATA_HOME` lus à la main | `noyau::systeme` (crate `dirs`) : les mêmes chemins sous Linux, AppData et le dossier Vidéos sous Windows | `noyau::systeme` |
| programmes lancés par `Command::new` | `noyau::systeme::commande` : celui posé à côté du Monteur d'abord (le paquet y mettra ffmpeg et whisper), sans fenêtre de console sous Windows | partout |
| verrou lu dans `/proc` seul | en plus, le fichier reste verrouillé par le système (`File::try_lock`) : sous Windows c'est lui qui compte, et un plantage le lâche | `noyau::verrou` |

**VerdierCAM introuvable** : Filmer VerdierCAM ouvre le dialogue « Où est VerdierCAM ? » et note le
choix dans `monteur/verdiercam`. Ordre de recherche : `MONTEUR_VERDIERCAM`, ce choix, à côté du
Monteur (`VerdierCAM.exe` ou `VerdierCAM.AppImage`), puis `~/VerdierCAM/programmes/` sous Linux.

**Linux seulement, pour l'instant** : le nanoKONTROL2 (séquenceur d'ALSA) et Meta+F12 (portail
GlobalShortcuts) ; ailleurs, ils se taisent sans erreur. Filmer passe encore par
`gpu-screen-recorder` et ses signaux : c'est le lot 3. Sous Windows, la version optimisée n'ouvre
pas de console (`windows_subsystem`) ; les plantages vont dans `monteur2-plantages.log`.

## Essais

- 70 contrôles passent (`cargo test --workspace --release`, sortie 0). Nouveaux : chemins et
  programmes, bips, rééchantillonnage, crêtes, JPEG à plat sur blanc, verrou tenu par le système
  (saboté une fois : il rougit).
- La fenêtre dans un KWin virtuel (`outils/voir-fenetre.sh`), avec un faux enregistreur qui pose
  une vidéo à deux pistes : vignette écrite (JPEG 1280 × 720 valide), décompte joué, vu-mètre de
  −53 à −28 dB sur le vrai micro, prise conclue avec son son mêlé en une piste, puis mise à la
  corbeille (celle du dossier jetable, avec ses `.trashinfo`). nanoKONTROL2 relié, raccourci prêt.
- Le dialogue de fichiers : avec un `HOME` jetable, Filmer VerdierCAM ouvre « Où est VerdierCAM ? »,
  le dialogue de KDE passé par le portail, dans le KWin virtuel (le déroulé ne le pilote pas plus loin).
- Le chemin du socket de KWin est limité à 108 octets : le dossier passé à `voir-fenetre.sh` doit
  être court.
- `voir-fenetre.sh` donne désormais l'écran du KWin virtuel aux services lancés par D-Bus : avant,
  le portail héritait du vrai écran, et ce dialogue s'est ouvert une fois sur le bureau de Christophe.

**Pas essayé** : la compilation pour Windows, qui demande la chaîne de VerdierCAM et la cible Rust
de Windows (lot 2).
