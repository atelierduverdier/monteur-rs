#!/usr/bin/env python3
"""Fait calculer au Monteur 1 (PySide6) ce que le noyau Rust doit retrouver à l'identique, sur
les fichiers de `crates/noyau/tests/donnees/`. Écrit `reference.json` à côté d'eux.

python3 outils/reference_python.py [~/Projets/logiciels/monteur]
Ne lit que les données de contrôle ; n'écrit que reference.json.
"""
import json
import sys
from dataclasses import asdict
from pathlib import Path

ICI = Path(__file__).resolve().parent.parent / "crates" / "noyau" / "tests" / "donnees"
sys.path.insert(0, str(Path(sys.argv[1] if len(sys.argv) > 1 else "~/Projets/logiciels/monteur").expanduser()))
import montage as mt  # noqa: E402
import noyau  # noqa: E402

ref = {"montages": {}, "journal": {}, "projet": {}, "fiches": {}, "description": {}}

for chemin in sorted(ICI.glob("* montage.json")):
    m = mt.lire(chemin)
    cles = mt._cles_zoom(m.zooms)
    instants = [m.duree * k / 997 for k in range(998)]
    ref["montages"][chemin.name] = {
        "reecrit": mt.en_dict(m),
        "vide": m.vide(),
        "coupes_actives": m.coupes_actives(),
        "accelerations": m.accelerations(),
        "duree_montee": m.duree_montee(),
        "sous_titres_a_part": bool(m.sous_titres_a_part()),
        "vitesses": [m.vitesse_a(t) for t in instants],
        "dans_une_coupe": [m.dans_une_coupe(t) for t in instants],
        "cles": cles,
        "etats": [mt.etat_zoom(m.zooms, t, cles) for t in instants],
        "rects": [mt.rect_source(*mt.etat_zoom(m.zooms, t, cles), m.largeur, m.hauteur) for t in instants],
        "temps_montes": [mt.temps_monte(m.coupes_actives(), t) for t in instants],
        "instants": instants,
    }

for chemin in sorted(ICI.glob("gestes *.jsonl")):
    j = mt.lire_journal(chemin)
    video = ICI / "2026-11-02 10-06-06.mp4"   # un nom d'OBS : le fichier n'a pas à exister
    ref["journal"][chemin.name] = {
        "debut": j.debut, "nb": len(j.evenements), "premiers": j.evenements[:5],
        "derniers": j.evenements[-3:], "taille_fenetre": list(j.taille_fenetre),
        "debut_video": mt.debut_de_la_video(video, 94.6),
        "decalage": mt.decalage_journal(j, video, 94.6),
        "trouve": mt.trouver_journal(video, 94.6, [ICI]).chemin.name,
    }

for chemin in sorted(ICI.glob("*.monteur")):
    p = noyau.lire_projet(chemin)
    ref["projet"][chemin.name] = {
        "video": str(p.video), "taille": p.taille, "enregistre_le": p.enregistre_le,
        "fiche": asdict(p.fiche), "description": asdict(p.description),
        "montage": mt.en_dict(mt.depuis_dict(p.montage)) if p.montage else None,
    }

r = noyau.Registre(ICI / "fiches.json")
ref["fiches"] = {"dossier": str(r.dossier), "rangement": str(r.rangement),
                 "projets": str(r.projets) if r.projets else "",
                 "fiches": {k: asdict(f) for k, f in r.fiches.items()}}

for chemin in sorted(ICI.glob("* description YouTube.txt")):
    ref["description"][chemin.name] = asdict(noyau.lire_description(chemin))

(ICI / "reference.json").write_text(json.dumps(ref, ensure_ascii=False, indent=1), encoding="utf-8")
print("reference.json :", sum(len(v) for v in ref.values() if isinstance(v, dict)), "entrées")
