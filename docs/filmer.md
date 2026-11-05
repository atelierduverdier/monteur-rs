# Filmer sans OBS (05/11/2026)

Bouton **Filmer…** du ruban : le Monteur lance `gpu-screen-recorder`, qui passe par le portail de
KDE, le même dialogue qu'OBS pour choisir la fenêtre. Le choix est gardé dans
`~/.local/share/monteur/portail-capture` ; Fichier › Filmer une autre fenêtre l'oublie.
**Filmer VerdierCAM…** lance VerdierCAM puis, 4 s plus tard, l'enregistrement : le dialogue ne liste
que les fenêtres déjà ouvertes.

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
