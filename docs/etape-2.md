# Étape 2 — le composeur (04/11/2026)

`crates/composeur` dessine l'image montée d'un instant — zoom, mise en lumière du réglage cliqué,
cercles des clics, étiquettes (réglage, boîte, atelier, raccourci), passage accéléré, sous-titres —
et la vignette YouTube. Repris trait pour trait de `compositeur.py` et de la charte du Monteur 1.

## Choix : le processeur dessine, pas de shader

Le plan prévoyait des shaders wgpu. Mesuré d'abord (`cargo run --release -p composeur --example
chrono`, 1920 × 1200) :

| | un fil | 32 fils |
|---|---|---|
| image entière (zoom compris) | 8 à 25 ms | **987 images/s** |
| calque seul (ce qui se pose sur l'image) | 0,2 à 5 ms | — |

Le rendu d'une prise de 30 min (54 000 images) se dessine donc en moins d'une minute au
processeur : la carte graphique n'y gagnerait rien. Pour l'aperçu, ImGui affiche l'image de la
vidéo en ne prenant que la part zoomée (coordonnées de texture) et le processeur peint le calque
par-dessus. Un contrôle vérifie que calque à part et image peinte d'un coup donnent le même
résultat (2/255 au plus). Le dessin est fait avec `tiny-skia` (chemins anticrénelés, comme
QPainter) ; le texte est mis en forme par `rustybuzz` (HarfBuzz, comme Qt) et tracé en chemins.

## Contrôle contre le Monteur 1

`QT_QPA_PLATFORM=offscreen python3 outils/reference_composeur.py` fait dessiner au Monteur 1
cinq instants de la prise du 04/11 à 07:59 (réglage cliqué et zoomé, atelier pendant son bond,
boîte qui s'ouvre, passage accéléré, raccourci) et quatre sous-titres incrustés, dont la phrase la
plus longue (deux lignes), et mesurer ses polices. `cargo test -p composeur` (8 contrôles) :

- **Polices** : ascendante, descendante, hauteur identiques à `QFontMetricsF` ; avances à 0,22 px
  au plus.
- **Images montées** : au plus 0,018 % des points s'écartent de plus de 10 % (la source seule :
  0,4 à 62 %), écart moyen 0,5/255 au plus. Seuil 0,03 % par image.
- **Sous-titres** : Qt dessine le texte qu'il ne transforme pas avec les glyphes de FreeType, que
  le hinting cale sur la grille et épaissit ; nous avec des chemins, comme Qt le texte agrandi.
  Mesuré : 0,08 à 0,35 % des points (bords de lettres), 2,4 à 3 % d'encre en moins, bords du texte
  au pixel près sauf le haut (2 px : le hinting monte les capitales). Contrôlés : les bords du
  texte et la teinte du fond de la boîte. Deux pistes écartées par la mesure : poser chaque lettre
  sur un pixel entier (pire, 1,1 px) ; la hauteur de ligne de Qt, elle, était arrondie au pixel du
  dessus (61 pour 60,48) : reprise.
- **Vignette** contre la vignette MACHINE du 23/10/2026 (ImageMagick) : même raison, 4 953
  points de bord diffèrent ; la charte, elle, est tenue — pour chaque bande (logo et nom, titre,
  sous-titre, trait, trois lignes) bords à 1 px, centre à 0,2 px (0,8 px pour « Genmitsu 4040 PRO »,
  qu'ImageMagick espace un peu autrement), encre à 1,3 %. Un décalage d'un pixel dans chacun des
  quatre sens sort de la charte. Le crénage d'Inter est appliqué (sans : plus loin d'ImageMagick).

Sabotés une fois chacun, tous vus : ombre de la mise en lumière, rebond, durée du clic, fondu,
marge des étiquettes, largeur et place des sous-titres, fond de leur boîte, corps du sous-titre de
la vignette.
