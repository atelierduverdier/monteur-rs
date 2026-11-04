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

# La préparation entière, comme « Préparer le montage » du Monteur 1, sur la prise du 02/11 à
# 10:06 : le mouvement de l'image et les silences sont mesurés par ffmpeg sur la vraie vidéo (lue
# seulement) et gardés ici, pour que le contrôle Rust tourne sans elle.
import copy  # noqa: E402
import subprocess  # noqa: E402
PRISES = [("2026-11-02 10-06-06", "gestes 2026-11-02 10-05-56.jsonl"),
          ("2026-11-04 07-59-21", "gestes 2026-11-04 07-42-17.jsonl")]
ref["preparations"] = {}
for nom, nom_journal in PRISES:
    VIDEO = Path.home() / "Vidéos" / "Copies d'écran" / f"{nom}.mp4"
    if not VIDEO.exists():
        continue
    depart = mt.lire(ICI / f"{nom} montage.json")
    m = copy.deepcopy(depart)
    gris = subprocess.run(mt.commande_image_grise(VIDEO), capture_output=True, check=True).stdout
    mouvement = mt.mouvement_image(gris)
    silences_txt = subprocess.run(mt.commande_silences(VIDEO), capture_output=True, text=True).stderr
    silences_txt = "\n".join(l for l in silences_txt.splitlines() if "silence_" in l)
    j = mt.lire_journal(ICI / nom_journal)
    dec0 = mt.decalage_journal(j, VIDEO, m.duree)
    clics = mt.instants_actifs(j, dec0, gestes_seuls=True)
    trouve = mt.calage_automatique(mouvement, clics)
    if trouve is not None and m.reglages.calage_auto:
        m.reglages.decalage_journal = trouve
    mt.analyser_journal(j, m, VIDEO, mouvement)
    mt.caler_etiquettes_sur_la_voix(m)
    silences = mt.lire_silences(silences_txt, m.duree)
    dec = mt.decalage_journal(j, VIDEO, m.duree) + m.reglages.decalage_journal
    actifs = mt.instants_actifs(j, dec)
    nouvelles = mt.temps_morts(silences, actifs, mouvement)
    m.coupes = [c for c in m.coupes if c.origine != "temps mort"] + nouvelles
    m.trier()
    ref["preparations"][nom] = {
        "journal": nom_journal,
        "mouvement": mouvement, "silences_txt": silences_txt, "clics": clics,
        "calage": trouve, "silences": silences, "actifs": actifs,
        "sans_journal": [asdict(c) for c in mt.temps_morts(silences, None, mouvement)],
        "changements": mt.changements_d_ecran(mouvement),
        "montage": mt.en_dict(m),
        "sans_mouvement": mt.en_dict(mt.analyser_journal(j, copy.deepcopy(depart), VIDEO, None)),
    }

# Les sous-titres : découpage, .srt (incrusté ou décalé des 3 s de vignette), images gardées.
ref["srt"] = {}
for chemin in sorted(ICI.glob("* montage.json")):
    m = mt.lire(chemin)
    ref["srt"][chemin.name] = {
        "texte": mt.texte_srt(m), "decale": mt.texte_srt(m, 3.0),
        "decoupes": [mt.decouper_sous_titre(st.texte) for st in m.sous_titres],
        "fusion": [asdict(s) for s in mt.fusionner_sous_titres(m.sous_titres)],
        "son_retire": mt.son_retire(m),
        "gardees": [mt.image_gardee(m, k / 30, 30) for k in range(int(m.duree * 30))],
    }
# Une transcription de whisper-cli, faite à la main avec ses pièges.
whisper = {"transcription": [
    {"text": " Bon,  on passe", "offsets": {"from": 1000, "to": 2400}},
    {"text": " dans l'atelier.", "offsets": {"from": 2500, "to": 2900}},
    {"text": "[BLANK_AUDIO]", "offsets": {"from": 3000, "to": 4000}},
    {"text": "sans temps"},
    {"text": " Et là on règle la profondeur de chaque passe", "offsets": {"from": 5000, "to": 8000}},
    {"text": " finie.", "offsets": {"from": 8100, "to": 8500}},
    {"text": " 10 mm.", "offsets": {"from": 9800, "to": 10200}},
    {"text": " La suite tout de suite après", "offsets": {"from": 10300, "to": 12500}}]}
(ICI / "whisper-essai.json").write_text(json.dumps(whisper, ensure_ascii=False), encoding="utf-8")
ref["whisper"] = [asdict(s) for s in mt.lire_whisper(ICI / "whisper-essai.json")]

(ICI / "reference.json").write_text(json.dumps(ref, ensure_ascii=False, indent=1), encoding="utf-8")
print("reference.json :", sum(len(v) for v in ref.values() if isinstance(v, dict)), "entrées")
