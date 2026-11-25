# Filmer sans OBS (05/11/2026)

Sous Linux, ci-dessous. Sous Windows, une autre capture, même résultat : [windows.md](windows.md), « Filmer sous
Windows ».

Bouton **Filmer** du ruban : le Monteur lance `gpu-screen-recorder`, qui passe par le portail de
KDE, le même dialogue qu'OBS pour choisir la fenêtre, à chaque fois (`-restore-portal-session no`). Jusqu'au
23/11/2026 le choix était gardé et repris sans rien demander : Christophe a filmé deux fois la mauvaise fenêtre.
Le dialogue propose aussi l'écran entier (« Partager « DP-1 » ») et une zone : vu le 25/11/2026 dans un KWin
virtuel, quand Christophe demandait le bureau complet pour filmer plusieurs fenêtres.
**Filmer VerdierCAM…** lance VerdierCAM puis, 4 s plus tard, l'enregistrement : le dialogue ne liste
que les fenêtres déjà ouvertes. Un VerdierCAM déjà ouvert (lu dans `/proc`, les captures d'essai `--capture` mises
à part) n'est pas relancé : le dialogue s'ouvre tout de suite, et le message dit si ses gestes sont notés (sa
variable `VCAM_JOURNAL_VIDEO` vise le dossier des gestes du Monteur).

**Enregistrement simple** (menu Fichier depuis le 23/11/2026 : dans le ruban, il doublait Filmer) : sans décompte, dans Copies d'écran.

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

## La webcam (25/11/2026)

Christophe : « l'incrustation d'une webcam […] que l'on peut déplacer la petite fenêtre, agrandir », et la passer en
grand « pour effacer son travail, histoire de dire quelque chose aux gens ». Fichier › Filmer aussi la webcam
(gardé dans `monteur/webcam`) : `webcam.rs` lance un second ffmpeg en même temps que gpu-screen-recorder, sur la
caméra V4L2 (`/dev/videoN` d'index 0), en MJPEG 1280 × 720 à 30 i/s si elle le sait (la C920 de l'atelier, oui),
encodée comme l'enregistreur de Windows (x264 « veryfast » ici), dans un Matroska à part.

**Le calage** : `-use_wallclock_as_timestamps` donne à chaque image l'heure de son arrivée, et ffmpeg annonce au
départ celle de la première (« start: … ») ; la vidéo écrite, elle, commence à 0. La prise sait l'heure de son
instant 0 (son `.ts`, plus le décompte) : la différence est le `decalage` du montage. Mesuré : la C920 donne sa
première image 0,37 s après le lancement. `-copyts` aurait gardé l'heure dans le fichier, mais le Matroska en sort
cassé (essayé le 25/11).

À l'arrêt, `q` sur l'entrée de ffmpeg ; la vidéo prend le nom `<prise> webcam.mkv` (la liste l'ignore, la corbeille
l'emporte avec sa prise), et le montage reçoit `webcam` : fichier, décalage, place et taille de la vignette, passages
en grand. Le composeur la peint sur l'écran et ses étiquettes, sous les sous-titres ; le rendu lit une image de la
webcam par image de la prise, gardée ou non, pour rester calé à travers les coupes. Vu le 25/11 dans un KWin
virtuel, avec un faux gpu-screen-recorder (une mire) et la vraie C920 : prise de 12,5 s, webcam de 12,0 s, décalage
2,6 s ; et sur une mire à compteur, la vignette, sa montée et le plein cadre tombent à la même seconde dans
l'aperçu et dans le rendu.

**En grand dès le tournage** (25/11/2026, l'option de la proposition : « Oui, ajoute-la ») : Meta+F11 (portail
GlobalShortcuts, à côté de Meta+F12 ; Windows+F11 par `RegisterHotKey`), la voie 5 S du nanoKONTROL2 ou un clic
sur le témoin notent l'heure de chaque bascule ; à l'arrêt, `Webcam::passages_des_bascules` en fait des passages
en secondes de la prise (en grand, vignette, en grand… ; restée en grand : jusqu'à la fin ; pendant le décompte :
0). Vu dans un KWin virtuel par deux appuis simulés sur la voie 5, à 3 s l'un de l'autre : passage de 2,733 à
5,735 s dans le montage. La touche elle-même n'a pas été pressée ici.

**La notification muette** (`notification.rs`, Christophe : « notification muette », plutôt qu'un bip qui se
serait enregistré avec le son de l'ordinateur) : `org.freedesktop.Notifications` avec `suppress-sound`, la bulle
d'avant remplacée ; sous Windows, la bulle d'une icône de la zone de notification (`NIIF_NOSOUND`), retirée à la fin
de la prise. Acceptée par le bureau de KDE le 25/11 (essai par gdbus, mêmes paramètres).

**Meta+F11 chez Christophe, le 25/11** : KDE a bien la touche pour le Monteur (kglobalaccel, composant actif), et
le raccourci déclenché par D-Bus arrive au Monteur ; mais la touche pressée n'a pas fait réagir KDE (aucun
`globalShortcutPressed` à l'écoute), elle est allée à la fenêtre de Claude, qui s'est mise en plein écran (F11 de
Chromium). Cause encore inconnue.

## Effacer une prise

Clic droit dans la liste, ou Fichier › Mettre la prise à la corbeille : une confirmation donne tous
les fichiers qui partent avec elle (`fichiers_lies`), puis la corbeille du système (crate `trash`,
celle de KDE sous Linux, celle de Windows sous Windows). Jamais de suppression définitive. Refusé
pendant un rendu ou un envoi. Le déroulé d'essai pose `XDG_DATA_HOME` : la corbeille est alors celle
du dossier jetable, pas la vraie.

