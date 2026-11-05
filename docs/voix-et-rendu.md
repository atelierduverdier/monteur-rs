# Nettoyer la voix, rendre (05/11/2026)

## Nettoyer la voix

Bouton Voix… du ruban, ou Montage › Nettoyer la voix. Les réglages valent pour toutes les prises,
dans `~/.local/share/monteur/voix.json`. Chaîne, dans l'ordre : coupe du grave, bruit, sifflantes,
compresseur, puis niveau. La même sert au rendu et à l'écoute des 10 s autour de la tête.

Bruit : `afftdn` de ffmpeg et non RNNoise comme le prévoyait la maquette : il tient dans la même
chaîne de filtres et ne demande aucun modèle. Sur la prise du 05/11 à 08:20, à 16 dB : silences de
−68,4 à −75,9 dB, voix inchangée (−26,3 à −26,7 dB). Au-delà de 16 dB, presque rien de plus.

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

