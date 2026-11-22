#!/usr/bin/env python3
"""CHAQUE SYMBOLE IMPORTÉ EST-IL EXPORTÉ ? (02/10/2026 — le premier lancement sous Windows).

Christophe, sous Windows (KVM) : « Le point d'entrée de procédure _Unwind_Resume est introuvable dans
libpugixml.dll ». Toutes les DLL étaient là ; c'est un SYMBOLE qui manquait. pugixml (MSYS2) l'importe de
libc++.dll ; le paquet embarquait la libc++.dll du compilateur llvm-mingw, qui la laisse à libunwind.dll.
Trouver chaque DLL ne suffit pas : ce contrôle, au paquetage, ouvre chaque exécutable et chaque DLL du dossier,
et pour chaque DLL qu'il importe ET qui voyage avec lui, exige que chaque nom importé y soit exporté. Les DLL du
système (KERNEL32, l'UCRT…) ne voyagent pas : elles ne sont pas vérifiées.

    python3 outils/verifier-dll-windows.py <llvm-objdump> <dossier du paquet>

Sortie 0 si tout est trouvé ; sinon la liste des manques, sortie 1.

Repris tel quel de VerdierCAM (verdiercam-imgui/outils/verifier-dll-windows.py) le 22/11/2026 : la
construction du Monteur ne dépend pas de ce dépôt-là.
"""
import os
import subprocess
import sys


def lire(objdump, chemin):
    """Les importations (DLL → noms) et les exportations (noms) d'un fichier PE."""
    texte = subprocess.run([objdump, "-p", chemin], capture_output=True, text=True, check=True).stdout
    imports, exports = {}, set()
    courant, dans_exports = None, False
    for ligne in texte.splitlines():
        brut = ligne.strip()
        if brut.startswith("DLL Name:"):
            courant = brut.split(":", 1)[1].strip().lower()
            imports.setdefault(courant, set())
            dans_exports = False
            continue
        if brut == "Export Table:":
            dans_exports, courant = True, None
            continue
        if not brut:
            courant = None
            continue
        morceaux = brut.split()
        if courant is not None and len(morceaux) >= 2 and morceaux[0].isdigit():
            imports[courant].add(morceaux[1])          # « hint  nom »
        elif dans_exports and len(morceaux) >= 3 and morceaux[0].isdigit() and morceaux[1].startswith("0x"):
            exports.add(morceaux[2])                   # « ordinal  RVA  nom »
    return imports, exports


def main():
    objdump, dossier = sys.argv[1], sys.argv[2]
    fichiers = {f.lower(): os.path.join(dossier, f) for f in os.listdir(dossier) if f.lower().endswith((".dll", ".exe"))}
    lus = {nom: lire(objdump, chemin) for nom, chemin in fichiers.items()}
    manques = []
    for nom, (imports, _) in sorted(lus.items()):
        for dll, noms in sorted(imports.items()):
            if dll not in lus:
                continue   # une DLL du système
            exportes = lus[dll][1]
            for n in sorted(noms - exportes):
                manques.append(f"{nom} : « {n} » introuvable dans {dll}")
    for m in manques:
        print("MANQUE  " + m)
    print(f"{len(lus)} fichiers, {len(manques)} symbole(s) importé(s) sans exportation")
    return 1 if manques else 0


if __name__ == "__main__":
    sys.exit(main())
