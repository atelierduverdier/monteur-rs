# Nettoyer la voix, rendre (05/11/2026)

## Nettoyer la voix

Bouton Voix… du ruban, ou Montage › Nettoyer la voix. Les réglages valent pour toutes les prises,
dans `~/.local/share/monteur/voix.json`. Chaîne, dans l'ordre : bruit (DeepFilterNet, avant ffmpeg), coupe
du grave, sifflantes, compresseur, puis niveau. La même sert au rendu et à l'écoute des 10 s autour de la tête.

Bruit, depuis le 26/11/2026 : **DeepFilterNet 3** (crate `debruitage`, libDF de Rikorose/DeepFilterNet, moteur
tract), choisi par Christophe à l'écoute contre `afftdn` et RNNoise, sur 45 s de « premier usinage » : `afftdn`
ne changeait presque rien au bruit d'atelier (fond à −41,7 dB contre −41,5 avant), DeepFilterNet le retire entre
les mots (−79 dB). Le son de la prise est lu par ffmpeg en 48 kHz, ses deux canaux moyennés, débruité par blocs de
10 ms, le retard du modèle (30 ms) rattrapé, et écrit dans un WAV mono à côté du rendu
(`….partiel.mp4.voix.wav`, effacé après) ; ce WAV prend la place du son de la prise dans le graphe du rendu.
L'écoute avant/après fait de même sur ses 10 s, dans un WAV jetable.

- Le modèle (8 Mo) est compilé dans le programme (`default-model` de libDF) : rien à télécharger, Windows compris.
- Seuils de libDF : ceux de l'outil `deep-filter` (−15, 35, 35 dB). Ceux de `RuntimeParams::default_with_ch`
  (−10, 30, 20) sautent des étages du modèle sur la voix : elle sortait 5 dB plus bas, corrélée à 0,83 avec la
  version Python validée, contre 0,998 avec les bons, au même niveau et sans décalage.
- Force : `bruit_force` de 0 à 1 donne l'atténuation maximale (`atten_lim` de libDF) de 6 à 60 dB, sans limite à
  100 %. Après toute la chaîne (compresseur et niveau compris), le fond de l'essai : −30 dB sans débruitage,
  −62 dB à 50 %, −68 dB à 75 %, −71 dB à 100 %, la voix à −11,5 dB.
- Vitesse : sur un seul cœur du Ryzen 9 7945HX, 12 s pour 6 min de son ; une prise de 40 min, 1 min 20.
- Secours : si DeepFilterNet échoue (prise sans son lisible…), `afftdn` reprend sa place dans la chaîne ffmpeg,
  comme avant : `Voix::filtres(false)`.
- En debug, tract 0.21.4 non optimisé échoue à préparer le modèle : ses crates sont optimisés même en debug
  (`[profile.dev.package]` du `Cargo.toml`). Les 0.21 suivantes ont changé l'API qu'utilise libDF : épinglé.

Avant, `afftdn` de ffmpeg et non RNNoise comme le prévoyait la maquette : il tenait dans la même chaîne de filtres
et ne demandait aucun modèle. Sur la prise du 05/11 à 08:20, à 16 dB : silences de −68,4 à −75,9 dB, voix
inchangée (−26,3 à −26,7 dB) ; c'était une prise au bureau, sans le bruit de l'atelier.

Niveau : le gain mesuré puis un limiteur. Le second passage de `loudnorm` basculait en mode
dynamique à cause des crêtes et ratait la cible (−16,9 LUFS, crête +2,2 dB). Maintenant, sur une prise
OBS : −14,2 LUFS, crête −0,7 dB. Une prise à la voix d'un seul côté reste à −16 LUFS : le limiteur y
travaille trop. Les prises filmées depuis la correction du micro n'ont pas ce cas.

« Baisser la musique » attend une piste musique.

## Rendre

Bouton Rendre… du ruban : cinq préréglages, écrits dans `noyau::prereglages`, et une file d'attente.
Le montage et la voix sont figés quand le rendu entre dans la file.

| Préréglage | Sortie | Essai sur 1:13 |
|---|---|---|
| YouTube | ` montage YouTube.mp4`, comme avant, vignette en tête | 1920 × 1168 |
| YouTube 1440p | même fichier, agrandi | non essayé |
| Court vertical | ` court.mp4`, 60 s au plus | 1080 × 1920, 60 s |
| Brouillon | ` brouillon.mp4` | 1184 × 720 |
| Extrait GIF | ` extrait.gif`, sans son, 15 s au plus | 800 × 486, 15 s |

Le court garde le centre de l'image : bon pendant un zoom, qui y met le sujet, à revoir ailleurs.
Le rendu reste en x264 : la carte graphique et « enregistrer comme préréglage » de la maquette ne
sont pas faits.

Les fenêtres n'ont pas pu être regardées à l'essai : la capture prend la fenêtre active, et Xvfb
n'a pas de présentation Vulkan.

## Fondus aux coupes

Réglage « Fondu aux coupes » de l'Inspecteur, 0,4 s d'office, 0 pour des coupes franches. À chaque
coupe, l'image d'avant continue dans ce qui est coupé pendant que celle d'après commence avant son
point de reprise : moitié de chaque côté du raccord, et la durée montée ne change pas. Le son baisse et
remonte le temps du fondu, sans se chevaucher. Le fondu se raccourcit si la coupe ou le passage voisin
sont courts ; il n'y en a pas au bord de la prise, ni contre une vidéo insérée ou un passage accéléré.

Le rendu seul : la visionneuse saute encore les coupes net.

