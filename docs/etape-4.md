# Étape 4 — le rendu (05/11/2026)

`crates/rendu`, bouton « Rendre la vidéo » (ou Fichier › Rendre la vidéo), ou sans fenêtre
`target/release/monteur2-rendu VIDÉO MONTAGE.json SORTIE.mp4`.

Même chaîne que le Monteur 1 : un ffmpeg décode, le composeur peint chaque image (le dessin de la
visionneuse), un ffmpeg encode en x264 crf 18 et AAC 192k, avec le son sans les coupes. La peinture
se fait sur 12 fils au plus ; les images sont remises en rang avant d'encoder. Écrit
`<vidéo> montage YouTube.mp4` en passant par `.partiel.mp4`, les sous-titres à part s'il y en a,
puis `<vidéo> avec vignette.mp4` si la vignette existe. Le rendu tourne en fond : on continue de
monter, avec l'avancement dans la barre d'état et « Arrêter le rendu ».

## Contre le Monteur 1

La prise du 02/11 à 10:06 (1 min 35), deux passages ajoutés (une coupe de 5 s, 12 s accélérées ×4),
rendue par les deux :

| | Monteur 2 | Monteur 1 |
|---|---|---|
| images | 2 392 | 2 392 |
| durée | 79,765 s | 79,765 s |
| temps de rendu | 19,6 s | 23,0 s |

Écart d'image : 48,2 dB en moyenne, 37,7 dB au pire (le texte : le hinting de l'étape 2). Son :
identique, écart 0,0 sur 7 657 440 échantillons. Le frein est maintenant x264 (122 images/s), plus la
peinture.

## Vidéos insérées

Essayé depuis la fenêtre sur une copie : une vidéo de 4 s intercalée à 12 s, une par-dessus à 20 s.
Durée 97,76 s (94,7 − 0,9 de coupe + 4), 100,76 s avec la vignette. Images vérifiées aux instants
attendus ; son mesuré : le 880 Hz de la vidéo intercalée à sa place, la voix sous la vidéo par-dessus.
Une intercalée plus courte que prévu garde sa dernière image : le son et l'image ne se décalent
jamais. Les sous-titres d'après une intercalée sont repoussés d'autant, dans l'image et le `.srt`.

## Longue prise

La prise du 04/11 à 15:50 (17 min 51 montées) : 32 137 images en 286 s (112 images/s). Pic de mémoire
4,1 Go, cache disque compris, contre 3,6 Go pour la prise de 1 min 35 : il ne grandit pas avec la
longueur.

## Lire pendant un rendu

Le rendu prend tous les cœurs : l'image de la visionneuse peut traîner de 60 à 130 ms sur le son tant
qu'il tourne (17 ms à vide). Essayé pour l'éviter, sans gain mesuré, et retiré : ffmpeg en priorité
basse (`nice`), puis les fils de peinture aussi. Le contrôle `lecture_au_rythme_de_l_horloge` se
lance donc sans rendu en cours. Le rendu lui-même n'y perd rien.

## Arrêter

« Arrêter le rendu » ne laisse ni `.partiel`, ni ffmpeg, et ne touche pas la vidéo déjà rendue.
