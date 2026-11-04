#!/usr/bin/env python3
"""Fait dessiner au Monteur 1 (compositeur.py, QPainter) quelques images montées d'une vraie prise,
et mesurer ses polices : ce que `crates/composeur` doit retrouver. Écrit dans
`crates/composeur/tests/donnees/` : source-N.png (l'image de la vidéo), attendu-N.png (l'image
montée), reference.json (instants, montage, mesures des polices).

QT_QPA_PLATFORM=offscreen python3 outils/reference_composeur.py [~/Projets/logiciels/monteur]
La vidéo n'est que lue.
"""
import json
import subprocess
import sys
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
ICI = RACINE / "crates" / "composeur" / "tests" / "donnees"
sys.path.insert(0, str(Path(sys.argv[1] if len(sys.argv) > 1 else "~/Projets/logiciels/monteur").expanduser()))
from PySide6.QtCore import QRectF  # noqa: E402
from PySide6.QtGui import QFontMetricsF, QGuiApplication, QImage, QPainter  # noqa: E402

import compositeur  # noqa: E402
import montage as mt  # noqa: E402
import noyau  # noqa: E402

app = QGuiApplication(sys.argv)
NOM = "2026-11-04 07-59-21"
VIDEO = Path.home() / "Vidéos" / "Copies d'écran" / f"{NOM}.mp4"
noyau_ref = json.loads((RACINE / "crates/noyau/tests/donnees/reference.json").read_text(encoding="utf-8"))
m = mt.depuis_dict(noyau_ref["preparations"][NOM]["montage"])


def actifs(t):
    et = [e for e in m.etiquettes if e.debut <= t <= e.fin]
    return {"reglage": any(e.genre == "reglage" and e.rect for e in et),
            "atelier": any(e.genre == "atelier" for e in et),
            "touche": any(e.genre == "touche" for e in et),
            "fenetre": any(e.genre == "fenetre" for e in et),
            "clic": any(c.t <= t <= c.t + 0.6 for c in m.clics),
            "sous_titre": any(s.debut <= t <= s.fin for s in m.sous_titres),
            "zoom": mt.etat_zoom(m.zooms, t)[0] > 1.05,
            "vitesse": m.vitesse_a(t) > 1}


def chercher(*voulu, apres=0.0, decalage=0.0):
    """Le premier instant où tout ce qui est voulu se voit (au pas de 0,1 s)."""
    t = apres
    while t < m.duree:
        a = actifs(t)
        if all(a[v] for v in voulu):
            return round(t + decalage, 3)
        t += 0.1
    raise SystemExit(f"aucun instant avec {voulu}")


instants = [
    chercher("reglage", "clic", "sous_titre", "zoom"),       # réglage cliqué, zoomé, sous-titré
    chercher("atelier", decalage=0.08),                       # un atelier, pendant son bond
    chercher("fenetre", "sous_titre"),                        # une boîte qui s'ouvre
    chercher("vitesse"),                                      # un passage accéléré
]
instants[1] = next(round(e.debut + 0.08, 3) for e in m.etiquettes if e.genre == "atelier")
touche = next((round(e.debut + 0.5, 3) for e in m.etiquettes if e.genre == "touche"), None)
if touche is not None:
    instants.append(touche)

ICI.mkdir(parents=True, exist_ok=True)
for i, t in enumerate(instants):
    png = subprocess.run(noyau.commande_image(VIDEO, t, m.duree), capture_output=True, check=True).stdout
    (ICI / f"source-{i}.png").write_bytes(png)
    source = QImage.fromData(png).convertToFormat(QImage.Format_RGB32)
    sortie = QImage(source.width(), source.height(), QImage.Format_RGB32)
    p = QPainter(sortie)
    compositeur.Compositeur(m).peindre(p, source, t, QRectF(0, 0, sortie.width(), sortie.height()))
    p.end()
    sortie.save(str(ICI / f"attendu-{i}.png"))

# Les sous-titres incrustés (éteints dans cette prise) : trois des instants, plus la phrase la plus
# longue, qui passe sur plusieurs lignes.
m.reglages.montrer_sous_titres = True
m.reglages.incruster_sous_titres = True
longue = max(m.sous_titres, key=lambda st: len(st.texte))
t_long = round((longue.debut + longue.fin) / 2, 3)
png = subprocess.run(noyau.commande_image(VIDEO, t_long, m.duree), capture_output=True, check=True).stdout
(ICI / f"source-{len(instants)}.png").write_bytes(png)
instants_st = [(0, instants[0]), (2, instants[2]), (3, instants[3]), (len(instants), t_long)]
for k, (i, t) in enumerate(instants_st):
    source = QImage(str(ICI / f"source-{i}.png")).convertToFormat(QImage.Format_RGB32)
    sortie = QImage(source.width(), source.height(), QImage.Format_RGB32)
    p = QPainter(sortie)
    compositeur.Compositeur(m).peindre(p, source, t, QRectF(0, 0, sortie.width(), sortie.height()))
    p.end()
    sortie.save(str(ICI / f"attendu-st-{k}.png"))
montage_st = mt.en_dict(m)
m.reglages.montrer_sous_titres = False

# Les mesures des polices, comme QFontMetricsF les donne.
textes = ["Profondeur de passe", "Usinage", "Ctrl+S", "▶▶ ×4", "Égalité", "Graver une image en aplats", "j'ai qû"]
mesures = []
for gras in (True, False):
    for px in (14, 38, 42, 46, 52, 34, 50, 47):
        f = compositeur.police(px, gras)
        fm = QFontMetricsF(f)
        mesures.append({"gras": gras, "px": px, "hauteur": fm.height(), "ascent": fm.ascent(),
                        "descent": fm.descent(), "avances": {s: fm.horizontalAdvance(s) for s in textes}})

(ICI / "reference.json").write_text(json.dumps({
    "prise": NOM, "instants": instants, "instants_st": instants_st, "montage_st": montage_st,
    "phrase_longue": longue.texte, "actifs": [actifs(t) for t in instants],
    "montage": mt.en_dict(m), "polices": mesures}, ensure_ascii=False, indent=1), encoding="utf-8")
print("instants :", instants)
