# Étape 3 — la fenêtre (04/11/2026, première moitié)

`target/release/monteur2 [dossier]` (sans dossier : celui des fiches du Monteur 1).

## Ce qui marche

- **Prises** : les vidéos du dossier et leur état (comme le Monteur 1, les ignorées en moins) ;
  un clic ouvre la prise. Les fiches ne sont que lues : le Monteur 1 reste celui qui les écrit.
- **Visionneuse** : l'image montée de l'instant, peinte par le composeur (le dessin du rendu),
  repeinte seulement quand quelque chose change — 10 ms à la taille du panneau, 60 images/s.
  Un zoom choisi, à l'arrêt : l'image entière et son cadre orange, qui se glisse ; la molette
  grossit. Lire, Couper avant, Couper après.
- **Ligne de temps** : règle, zooms, étiquettes, clics, sous-titres, forme de la voix, coupes
  (les passages coupés assombris). Un clic dans la règle place la tête ; un élément se choisit,
  se glisse, s'allonge par un bord ; glisser dans une piste vide en crée un ; double-clic : un
  élément de 2 s. Molette : défiler ; Ctrl+molette : resserrer.
- **Inspecteur** : l'élément choisi (début, fin, grossissement, centre, texte, genre, coupe ou
  accélération), sinon les réglages du montage.
- **Lecture** (`crates/media`) : image décodée en fond, son par `cpal`, horloge du son ; les
  passages coupés se sautent comme au rendu. Mesuré : saut de la tête en 21 à 60 ms, image à
  0,4/255 de celle de ffmpeg, image calée sur l'horloge à 17 ms près, 3,03 s lues en 3 s.
- **Touches** : Espace (lire), K (arrêter), J / L (±5 s), ← / → (une image, Maj : 1 s),
  Début / Fin, C puis C (une coupe), Z (un zoom), Échap, Suppr, Ctrl+Z, Ctrl+Maj+Z, Ctrl+S.
- **Préparer le montage** (menu Montage) : en fond, la même chaîne que le Monteur 1 (calage,
  gestes, temps morts). Sur une copie de la prise du 02/11 à 10:06 : 10 zooms, 6 étiquettes,
  gestes calés de −0,35 s — ce que le Monteur 1 trouve.
- **Annuler** par instantanés du montage (300 pas) ; le montage s'écrit seul une demi-seconde
  après le dernier changement, au format du Monteur 1.
- **Un seul Monteur à la fois** : le verrou est celui du Monteur 1, au même format. Essayé dans
  les deux sens : le Monteur 2 ouvert, le Monteur 1 ne prend pas le verrou ; le Monteur 1 ouvert,
  le Monteur 2 dit « Le Monteur est déjà ouvert. ».

## Remarques de Christophe, vidéo du 05/11/2026

Faites : liste des prises lisible (miniature, date, durée, état en pastille à droite) ; bouton
« Préparer le montage » en vue, dans un ruban ; boutons de lecture dessinés (début, −5 s, lire ou
pause, stop, +5 s, fin) ; le cadre du zoom se trace dans la visionneuse (`zoom_trace`, contrôlé) :
glisser dedans le déplace, un coin le retaille, tracer à côté en fait un autre ; « Nouveau zoom » (Z)
en pose un à la tête. Vus en plus sur la vidéo : les noms des champs de l'inspecteur étaient
coupés ; choisir un zoom loin de la tête montrait une autre image (la tête y vient maintenant) ; un
grossissement tapé sous 1 aurait arrêté le programme au déplacement du cadre.

Stop arrête et revient d'où la lecture est partie : en montant, on réécoute le même passage.

Décidé avec lui le 05/11/2026, et fait :

- **Un dossier par projet** (`~/Vidéos/Projets/<nom>/`, `noyau::projets`) : « Nouveau projet »
  y déplace la prise, ce qui l'accompagne et sa fiche ; le sélecteur en haut de la liste passe des
  prises d'OBS à un projet. Comme tout s'écrit à côté de la vidéo, le Monteur 1 ouvre ce dossier
  aussi.
- **Vidéos insérées** (`Montage.insertions`, piste Vidéos) : importées dans « Vidéos importées »
  du projet, l'originale restant en place ; **intercalée**, la prise s'arrête, la vidéo passe avec
  son son, la prise reprend ; **par-dessus**, elle remplace l'image, sans déformation, pendant que
  la voix et ses sous-titres continuent. Zooms, clics et étiquettes se taisent pendant ce temps : ils
  parlent de l'écran filmé. Le Monteur 1 ne les voit pas : un montage réenregistré par lui les perd.
- **Vignette** (onglet à côté de la visionneuse) : image prise à la tête, cadrage à la souris et à
  la molette, textes, débordements en rouge, PNG et JPG à côté de la vidéo, fiche mise à jour. Une
  vignette faite ou retouchée ailleurs n'est remplacée qu'au second clic.

Les fiches sont maintenant écrites par le Monteur 2 aussi ; un fichier illisible est mis de côté,
jamais écrasé. Le Monteur 2 ne rend pas encore : le rendu (étape 4) devra tenir les insertions.

## Remarques de Christophe, seconde vidéo du 05/11/2026

Faites : le menu Fichier tient Nouveau projet, Ouvrir un projet (les projets, Copies d'écran, un
fichier `.monteur`), Ajouter une prise au projet, Enregistrer (Ctrl+S : montage, fiches et
`<vidéo>.monteur`), un témoin « ● enregistré » dans la barre d'état ; la liste suit le dossier
toute seule ; « Filmer VerdierCAM… » est revenu ; le curseur des coins du cadre suit leur sens ; les
coupes sont hachurées et celle en cours grandit jusqu'à la tête ; les pistes remplissent le
panneau (jusqu'au double) ; la piste Vidéos montre la barre de la prise et ses images.

## Essayer sans souris

`MONTEUR2_SCENARIO="ouvrir 0; aller 20; capture /tmp/a.png; lire; attendre 3; dire; preparer;
dire; quitter"` déroule des gestes et écrit l'image de la visionneuse. Toujours sur un dossier
jetable avec `XDG_DATA_HOME` jetable : le montage s'écrit à côté de la vidéo.

## Reste à faire pour finir l'étape 3

Passage accéléré lu accéléré (il se lit pour l'instant à vitesse normale, le badge ×4 s'y voit),
sous-titres de whisper dans « Préparer », choisir un autre dossier, vignettes d'image sur la piste
Image, S (scinder) et M (marqueur : les chapitres arrivent avec le lot A).

## Fin de l'étape 3 (05/11/2026)

**Accéléré lu accéléré** : la tête passe `v` fois plus vite dans un passage accéléré ; le son reste à
sa vitesse et se recale à la sortie, comme au rendu, qui ne garde que le début du son du passage.
Mesuré à ×4 sur 140–170 s, lancé à 139 s : 147,9 s après 3 s, 160,0 s après 6 s, 170,5 s après 9 s.

**S** coupe en deux, à la tête de lecture, l'élément choisi : zoom, étiquette, coupe, vidéo
par-dessus, sous-titre (ses mots se partagent selon l'instant). Ni un clic ni une intercalée.

**M** pose un chapitre ; piste Chapitres (un trait et son titre) ; Montage › Proposer les chapitres
en met un par atelier visité d'après le journal de gestes, sans toucher à ceux posés à la main.

## Remarques de Christophe, vidéo du 05/11/2026 à 16:42

**La molette à la fin de la prise** : le son finissait 11 ms avant la durée de la prise, l'horloge se
figeait à 202,457 s sur 202,468, la lecture ne s'arrêtait jamais et la vue ramenait sans cesse la
tête à l'écran. La lecture s'arrête maintenant quand l'horloge ne bouge plus dans la dernière seconde,
et la vue laisse 3 s à celui qui défile à la molette.

**Les gestes de montage** passent du ruban à une barre au-dessus de la ligne de temps.

**Valider une coupe** : clic droit sur la ligne de temps, Entrée, C, ou le bouton « Valider la coupe ».

