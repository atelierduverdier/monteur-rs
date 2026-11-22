# Filmer sans OBS (05/11/2026)

Sous Linux, ci-dessous. Sous Windows, une autre capture, même résultat : [windows.md](windows.md), « Filmer sous
Windows ».

Bouton **Filmer…** du ruban : le Monteur lance `gpu-screen-recorder`, qui passe par le portail de
KDE, le même dialogue qu'OBS pour choisir la fenêtre. Le choix est gardé dans
`~/.local/share/monteur/portail-capture` ; Fichier › Filmer une autre fenêtre l'oublie.
**Filmer VerdierCAM…** lance VerdierCAM puis, 4 s plus tard, l'enregistrement : le dialogue ne liste
que les fenêtres déjà ouvertes.

**Enregistrement simple** ne reprend pas la fenêtre gardée : le dialogue s'ouvre à chaque fois, et ce
choix ne remplace pas celle de Filmer.

La vidéo s'écrit dans le dossier affiché sous `capture ….partiel.mp4`, que la liste ignore. À l'arrêt,
elle prend le nom d'OBS (`2026-11-05 07-47-00.mp4`) à l'instant de sa première image, lu dans le `.ts`
du programme : les gestes se calent sur ce nom. Jamais par-dessus une prise existante. Fermer le
Monteur en filmant termine la vidéo proprement.

Réglages, comme OBS : 30 i/s, curseur, AAC 192 kb/s. L'image est encodée en H.264 par la carte AMD.

**Le micro des deux côtés.** La Scarlett donne la voix sur le canal gauche seul : prise du 05/11 à
08:20, −33,0 dB à gauche, −101,0 dB à droite. Bureau et micro s'enregistrent donc sur deux pistes, et
à l'arrêt ffmpeg les mêle en une seule, le micro des deux côtés, la vidéo recopiée telle quelle
(`GRAPHE_SON`). Sur sa vraie voix : −32,5 dB à gauche comme à droite, le niveau d'origine. Le son du
bureau n'est pas baissé. Si le mélange échoue, la prise garde ses deux pistes et un message le dit.

Essai du 05/11, fenêtre VerdierCAM : 1 447 images en 48,2 s, écart 33,3 ms, 34,9 ms au plus ; texte
aussi net qu'OBS, à l'œil, à ×2 ; préparée et rendue avec vignette dans le Monteur. L'image suit la
taille de la fenêtre (1920 × 1168 ici), pas celle de l'écran.

Essais de la fenêtre : `MONTEUR2_ECRAN` remplace le programme ; un script qui appelle le vrai avec
`-w DP-1` évite le dialogue. Arrêt en 0,12 s.

## Le lot du 05/11/2026

**Volume égal** (Inspecteur, coché par défaut, écrit dans le montage seulement s'il est décoché) : le
rendu mesure le son monté puis l'amène à −14 LUFS, crêtes à −1 dB (`loudnorm` en deux passages).
Prise de 08:20 : −30,9 LUFS avant, −14,7 LUFS et −0,5 dB de crête après l'AAC.

**Panneau Texte**, à côté de l'Inspecteur : un clic va à la phrase, « Couper » pose une coupe
d'origine `texte` sur toute la phrase, « Rétablir » l'enlève, la recherche filtre, la phrase lue se
colore et reste en vue. Une phrase déjà dans une autre coupe se reprend dans la ligne de temps.

**À l'arrêt**, la prise s'ouvre et Préparer se lance seul.

**Vu-mètre** à côté du bouton Arrêter : le micro par défaut est lu à part (cpal, `media::sons`), jusqu'à la saturation en rouge ;
« Micro muet ? » au bout de 5 s sous −45 dB.

**Décompte** : à la première image, l'enregistrement se met en pause (SIGUSR2) le temps de trois bips,
puis repart. Mesuré : 2,1 s gardées sur 5 s avec 3 s de pause. Le nom de la prise compte ces 3 s,
sinon les gestes seraient décalés.

**Raccourci global** : Meta+F12 arrête, par le portail GlobalShortcuts. KDE fait confirmer la touche
au premier enregistrement. Le portail refuse une appli sans identité : `outils/installer.sh` pose le
`.desktop` `fr.atelierduverdier.Monteur2`, qui donne aussi le lanceur du menu. Identité acceptée le
05/11 ; l'appui lui-même reste à essayer.

## Effacer une prise

Clic droit dans la liste, ou Fichier › Mettre la prise à la corbeille : une confirmation donne tous
les fichiers qui partent avec elle (`fichiers_lies`), puis la corbeille du système (crate `trash`,
celle de KDE sous Linux, celle de Windows sous Windows). Jamais de suppression définitive. Refusé
pendant un rendu ou un envoi. Le déroulé d'essai pose `XDG_DATA_HOME` : la corbeille est alors celle
du dossier jetable, pas la vraie.

