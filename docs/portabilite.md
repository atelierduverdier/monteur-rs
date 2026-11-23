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

**Lot 2, la compilation pour Windows** : faite le même jour, voir [windows.md](windows.md).

# Lot 4 : les sous-titres et YouTube pour tout le monde (22/11/2026)

**Whisper livré.** `whisper-cli` se compile depuis les sources de whisper.cpp 1.9.5 (empreinte du tarball
`ff1a9053…`) : pour Windows avec le llvm-mingw de VerdierCAM, ses variantes du processeur chargées au lancement
(`GGML_BACKEND_DL`, de SSE4.2 à AVX-512, zen4 choisie seule ici) ; il part dans le zip avec ses DLL. Sous Wine :
25 s de voix transcrites en 10,8 s sur le processeur, en français. Le libc++ récent ne rend plus `<cstdlib>` ni
`<algorithm>` par ricochet : `-include` de l'un et de l'autre à la compilation, les sources ne bougent pas.

**Le modèle.** Le Monteur prend le meilleur présent (`noyau::sous_titres::trouver_modele`) : large-v3-turbo,
puis turbo q5_0, medium, small, base, dans ses modèles (`monteur/modeles`) puis dans ceux de la dictée. Montage ›
Télécharger le modèle des sous-titres propose turbo q5_0 et la détection de voix (575 Mo, huggingface.co) : curl
du système (Windows 10 et 11 l'ont), un `.partiel`, l'empreinte SHA-256 vérifiée avant de poser le fichier, rien
de retéléchargé s'il est déjà là ; l'avancement et « Arrêter » dans la barre d'état. Essayé pour de bon sur la
détection de voix (0,9 Mo) : `cargo test -p appli --release -- --ignored le_vad`.

**La langue.** Montage › Langue des sous-titres : français, anglais, espagnol, allemand, italien, ou reconnue
seule (`-l auto`). Gardée dans `monteur/langue-sous-titres`. Sans whisper ni modèle, Préparer le dit et dit quoi
faire.

**Préparer pour YouTube.** Sans `youtube-envoi` (il passe par le compte Google de Christophe), l'onglet YouTube
devient « Envoi par YouTube Studio » : Copier le titre, la description, les mots-clés ; Ouvrir YouTube Studio ;
Montrer la vidéo ; les noms de la vidéo montée et de la vignette à y glisser. Le presse-papiers est enfin relié au
système (`presse_papiers.rs`, arboard) : Ctrl+C et Ctrl+V marchent aussi dans les champs, qui ne parlaient qu'à
eux-mêmes. Chez Christophe, `youtube-envoi` est là : rien ne change.

# Lot 5 : tout le reste (22/11/2026)

- **Le nom public**, « Monteur VerdierCAM » (décidé par Christophe) : titre de la fenêtre, lanceur, paquets
  (`Monteur-VerdierCAM-<version>-…`), `Monteur VerdierCAM.exe` sous Windows. L'identité technique ne bouge pas
  (`fr.atelierduverdier.Monteur2`, `monteur2`) : le raccourci accordé par KDE, le verrou du Monteur 1 et les essais
  en dépendent.
- **L'icône et la fiche du `.exe`** (`ressources/monteur.ico`, tirée du SVG ; `ressources/monteur.rc` : nom, version,
  éditeur), compilées par windres et liées au programme ; la barre de titre la prend aussi (`vendor/dear-app`).
- **La licence** : GPL-3.0-or-later en clair (`LICENSE`), la marque VerdierCAM à part (`MARQUE.md`), l'adresse du
  code dans le LISEZMOI et « À propos ». Christophe rendra le dépôt public.
- **Aide › Ce qu'il faut** : chaque outil, trouvé ou non, et de quoi réparer (télécharger le modèle, choisir
  VerdierCAM). Elle s'ouvre seule au premier lancement, ou quand ffmpeg manque. **Aide › Mode d'emploi** ouvre
  `docs/mode-emploi.md` mis en page (`outils/mode-emploi-html.py`), livré dans les deux paquets. **À propos**.
- **La vignette à son nom** : panneau Vignette › Chaîne, le nom et le logo (PNG, ramené dans le carré de 60 px) de
  sa chaîne à la place de ceux de VerdierCAM, pour toutes les prises (`monteur/marque.json`). La marque d'office ne
  change pas un pixel (contrôlé contre le Monteur 1).
- **Le carton « Fait avec VerdierCAM »** : 2 s à la fin de la vidéo avec vignette, sur le fond du montage (le logo,
  la phrase, verdiercam.fr). Coché d'office dans les réglages du montage ; écrit dans `montage.json` seulement
  décoché, comme le fondu. Sans carton, la commande de montage reste celle du Monteur 1 (contrôlé). Essayé : 3 s de
  vignette + 8 s de prise + 2 s de carton = 13,0 s, l'image de travail du carton effacée.
- **Le paquet Linux** : [linux.md](linux.md).
- **La signature du paquet Windows** (23/11/2026), comme VerdierCAM et avec ses variables : chaque `.exe` et chaque
  DLL, dès qu'un certificat existe ([windows.md](windows.md), « La signature »).
