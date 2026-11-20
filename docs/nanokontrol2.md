# Le nanoKONTROL2 (20/11/2026)

Le Monteur 2 écoute le nanoKONTROL2 de Korg dès son lancement (`crates/appli/src/midi.rs`). Le
séquenceur d'ALSA le relie à un port « Monteur 2 » ; débranché puis rebranché, il doit se relier
seul (le Monteur suit les ports qui naissent et qui partent ; pas encore essayé en vrai). Son nom
s'affiche dans la barre d'état tant qu'il est relié.

Les messages arrivent **même quand une autre fenêtre est active** : ● arrête de filmer pendant qu'on
travaille dans VerdierCAM, sans passer par Meta+F12.

## L'appareil

Lu sur celui de l'atelier le 20/11/2026 (demande d'identité, puis de la scène en cours, rien
d'écrit) : Korg nanoKONTROL2, **mode CC**, canal 1, **voyants en mode interne**, scène d'usine :

| Contrôles | CC |
|---|---|
| Faders 1 à 8 | 0 à 7 |
| Boutons rotatifs 1 à 8 | 16 à 23 |
| S 1 à 8 / M 1 à 8 / R 1 à 8 | 32 à 39 / 48 à 55 / 64 à 71 |
| ▶ ■ ◀◀ ▶▶ ● CYCLE | 41 42 43 44 45 46 |
| TRACK ◀ ▶, MARKER SET ◀ ▶ | 58 59, 60 61 62 |

Le Monteur ne lui envoie rien. En mode interne, un bouton ne s'allume que pendant l'appui ; pour
que le Monteur allume ● pendant qu'on filme, il faudrait passer les voyants en mode externe (un
octet de la scène, écrit par SysEx) : pas fait.

## Ce que fait chaque contrôle

| Contrôle | Geste |
|---|---|
| ▶ | Lire / pause (Espace) |
| ■ | Stop : revient d'où la lecture est partie |
| ◀◀ / ▶▶ | −5 s / +5 s (J / L), répété tant qu'on tient |
| TRACK ◀ / ▶ | Une image en arrière / en avant, répété tant qu'on tient |
| CYCLE | La coupe : la commencer, puis la valider (C) |
| MARKER SET | Un chapitre à la tête (M) |
| MARKER ◀ / ▶ | Le chapitre d'avant / d'après ; le début ou la fin s'il n'y en a plus |
| ● | Filmer (comme le bouton Filmer…), ou arrêter de filmer |
| Voie 1 : S / M / R | Nouveau zoom (Z) / scinder (S) / supprimer l'élément choisi (Suppr) |
| Voie 1 : fader | Grossissement du zoom choisi, de 1,15 à 4 |
| Boutons rotatifs 1 / 2 | Centre du zoom choisi : gauche-droite / haut-bas |
| Voie 2 : S / M / R | Annuler / rétablir / Échap |
| Voie 3 : S / M / R | Préparer le montage / Nettoyer la voix… / Rendre… |
| Voie 4 : S / M / R | Enregistrer le projet / aller au début / aller à la fin |
| Faders 2 / 3 / 4 / 5 | Voix : réduction du bruit / coupe du grave / rapport d'égalisation / adoucir les « s » |
| Bouton rotatif 4 | Voix : seuil d'égalisation |
| Bouton rotatif 8 | Navette : au centre rien ; tourné, la tête file en arrière ou en avant, de 0,5 à 16 s par seconde |

Les faders de la voix ne marchent que la fenêtre Nettoyer la voix ouverte ; l'écoute s'y refait
comme au curseur, et rien ne se garde sans « Garder ces réglages ». Une vidéo intercalée qui passe :
▶ la met en pause, ■ revient à la prise.

**Reprise douce** : un fader qui n'est pas à la position du réglage ne le fait pas sauter. Il le
prend en passant par sa valeur ; d'ici là la barre d'état dit où aller (« passe par 45 % »). Tout un
mouvement de fader est **un seul pas** d'annulation.

**Navette** : le nanoKONTROL2 n'a pas de molette sans fin, ses boutons rotatifs vont de 0 à 127.
La navette arrête la lecture et déplace la tête toutes les 80 ms ; ▶ ou ■ l'arrêtent, le bouton
remis au centre aussi.

## Changer l'affectation

`~/.local/share/monteur/nanokontrol2.json`, lu au lancement, jamais écrit par le Monteur. Il ne
contient que ce qui change :

```json
{ "45": "rien", "39": "chapitre", "71": "filmer" }
```

Gestes : `lire`, `stop`, `reculer`, `avancer`, `image_precedente`, `image_suivante`, `debut`, `fin`,
`coupe`, `chapitre`, `chapitre_precedent`, `chapitre_suivant`, `filmer`, `zoom`, `scinder`,
`supprimer`, `echap`, `annuler`, `retablir`, `preparer`, `voix`, `rendre`, `enregistrer` (boutons) ;
`grossissement`, `centre_x`, `centre_y`, `bruit`, `grave`, `rapport`, `seuil`, `adoucir`, `navette`
(faders et boutons rotatifs). `"rien"` retire un contrôle. Une ligne fausse est dite dans la barre
d'état au lancement, le reste s'applique.

`MONTEUR2_MIDI` change le nom cherché (une autre surface en mode CC).

## Essai du 20/11/2026

La fenêtre dans un KWin virtuel, sur une copie de la prise du 18/11 à 07:45 (27 s) :

- reliée au vrai nanoKONTROL2 dès le lancement (`aconnect -l` : 24:0 → « Monteur 2 ») ; son nom
  dans la barre d'état ;
- un vrai message ALSA ▶ (CC 41, envoyé par `aseqsend`) : la lecture part de 5,0 s, à 10,3 s 5 s plus
  tard ;
- ■ revient à 5,0 s ; ▶▶ mène à 10 s, ◀◀ à 5 s ; TRACK ▶ tenu 1 s avance de 12 images (5,40 s) ;
- deux chapitres posés (5,4 et 20 s) ; MARKER ◀ depuis 25 s : 20 s, puis 5,4 s ; MARKER ▶ : 20 s ;
- S1 pose un zoom ×1,8, choisi. Fader 1 en bas : refusé, « passe par 36 % » ; passé par sa valeur :
  ×2,29, puis ×4 en haut. Bouton 1 : refusé à 0 et à 60, pris à 64 (centre 0,5039). Trois Ctrl+Z
  défont le centre, puis le grossissement, puis le zoom : un pas par geste ;
- fader 2, fenêtre Voix fermée : refusé avec son message ; M3 l'ouvre ; fader 2 en haut : bruit à
  100 % ;
- navette à fond à droite pendant 1 s : de 8,0 à 22,9 s, lecture arrêtée.

Pas encore essayé : un appui sur l'appareil lui-même, le débranchement à chaud, ● (filmer).

## Essayer sans l'appareil

- Le déroulé d'essai : `midi <cc> <valeur>` (127 appuie un bouton, 0 le relâche), `dire_voix`.
- Un vrai message ALSA, envoyé au port du Monteur **par son numéro** : `aseqsend -p 129:0 "B0 29 7F"`
  (`aconnect -l` le donne). Par son nom, il pourrait viser un autre Monteur ouvert.
