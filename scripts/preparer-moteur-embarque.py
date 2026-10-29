#!/usr/bin/env python3
"""Prepare le moteur de transcription EMBARQUE dans l'installateur.

Telecharge l'archive officielle de whisper.cpp, verifie son empreinte, et n'en extrait que le
jeu minimal necessaire dans `src-tauri/moteur/`, que Tauri embarque comme ressource.

⛔ **Pourquoi embarquer plutot que telecharger au premier lancement.** Oyant promet une dictee
entierement locale : une application qui doit appeler GitHub pour fonctionner *du tout*
contredirait sa propre promesse. Embarque, il marche a la sortie de l'installation, sans reseau,
sans verification d'empreinte a l'execution, sans ecran « moteur non installe ». Toute cette
surface d'echec quitte le chemin normal. Les dos d'execution acceleres, eux, restent optionnels.

⚠️ **Le contenu de `src-tauri/moteur/` n'est PAS versionne.** Des binaires d'un tiers dans notre
depot se perimeraient en silence et brouilleraient les differences. C'est ce script qui fait
autorite, et son empreinte attendue est le garde-fou.

⚠️ **Jeu minimal MESURE, pas devine** (tables d'import PE, le 2026-09-17) :
`whisper-cli.exe` depend de `whisper.dll` et `ggml.dll`, qui dependent de `ggml-base.dll`. Les
neuf `ggml-cpu-*.dll` n'apparaissent dans aucune table d'import parce qu'elles sont chargees **a
l'execution** selon le processeur : il faut donc les prendre toutes, sous peine d'un moteur qui
marche sur la machine de developpement et echoue ailleurs. Tout le reste de l'archive (SDL2, les
exemples, les binaires de test) est ecarte, ce qui fait passer 20,8 Mo a 10 Mo et retire au
passage une licence a porter.

⚠️ **`vcomp140.dll` n'est PAS dans l'archive amont** et n'est pas garanti sur une machine neuve :
il vient du redistribuable Visual C++. Le moteur ne demarre pas sans lui, et le message de
Windows ne le nomme pas. Oyant le detecte a l'execution et le dit ; voir `moteur.rs`.

Usage :
    python scripts/preparer-moteur-embarque.py            # prepare si besoin
    python scripts/preparer-moteur-embarque.py --verifier # sort en 1 s'il reste a faire

Idempotent : ne retelecharge rien si le moteur en place porte deja la bonne empreinte.
"""

from __future__ import annotations

import hashlib
import io
import shutil
import sys
import urllib.request
import zipfile
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
CIBLE = RACINE / "src-tauri" / "moteur"

# ⚠️ Etiquette de COMPILATION (`b5130`) et pas de version. Les etiquettes `vX.Y.Z` de whisper.cpp
# ne portent AUCUN binaire : `v1.9.4` et `v1.9.3` sont vides. Suivre « la derniere version »
# donnerait zero fichier, avec un message qui accuserait le reseau.
ETIQUETTE = "b5130"
ARCHIVE = "whisper-bin-x64.zip"
URL = f"https://github.com/ggml-org/whisper.cpp/releases/download/{ETIQUETTE}/{ARCHIVE}"
TAILLE = 8_573_270
EMPREINTE = "f9ec6c52a2e949b62ab51fa21d0d497958f9e41c3010c157c4e42932d5316f3c"
PREFIXE = "Release/"

# Le jeu minimal. Les `ggml-cpu-*.dll` sont prises par motif : en nommer neuf a la main serait
# neuf occasions d'en oublier une le jour ou l'amont en ajoute.
# `parakeet-cli.exe` et `parakeet.dll` (moins d'un mega-octet a eux deux) font que le modele
# Parakeet marche avec le moteur fourni, sans rien installer de plus.
FICHIERS = [
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "parakeet-cli.exe",
    "parakeet.dll",
]
MOTIF_CPU = "ggml-cpu-"

# L'avis de licence est RECUPERE a la source et jamais reconstitue : un avis legal approximatif
# ne remplit pas la condition qu'il pretend remplir.
URL_LICENCE = f"https://raw.githubusercontent.com/ggml-org/whisper.cpp/{ETIQUETTE}/LICENSE"
NOM_LICENCE = "LICENSE-whisper.cpp.txt"

MARQUE = "empreinte.txt"


def deja_prepare() -> bool:
    marque = CIBLE / MARQUE
    if not marque.is_file():
        return False
    if marque.read_text(encoding="utf-8").strip() != EMPREINTE:
        return False
    # La marque ne suffit pas : on verifie que les fichiers sont bien la.
    for nom in FICHIERS + [NOM_LICENCE]:
        if not (CIBLE / nom).is_file():
            return False
    return any(CIBLE.glob(f"{MOTIF_CPU}*.dll"))


def telecharger() -> bytes:
    print(f"Telechargement de {ARCHIVE} ({TAILLE / 1e6:.1f} Mo)...")
    with urllib.request.urlopen(URL, timeout=300) as reponse:
        octets = reponse.read()

    if len(octets) != TAILLE:
        raise SystemExit(f"Taille inattendue : {len(octets)} au lieu de {TAILLE}.")

    calcule = hashlib.sha256(octets).hexdigest()
    if calcule != EMPREINTE:
        raise SystemExit(
            f"Empreinte inattendue.\n  attendue : {EMPREINTE}\n  obtenue  : {calcule}"
        )
    print("Empreinte conforme.")
    return octets


def extraire(octets: bytes) -> None:
    if CIBLE.exists():
        # ⚠️ On vide avant : garder des fichiers d'une version anterieure a cote des nouveaux
        # ferait cohabiter deux jeux de bibliotheques, et celle qui se charge dependrait de
        # l'ordre de recherche de Windows.
        shutil.rmtree(CIBLE)
    CIBLE.mkdir(parents=True)

    pris: list[str] = []
    with zipfile.ZipFile(io.BytesIO(octets)) as archive:
        for entree in archive.infolist():
            if entree.is_dir() or not entree.filename.startswith(PREFIXE):
                continue
            nom = entree.filename[len(PREFIXE) :]
            if "/" in nom or "\\" in nom:
                continue
            if nom not in FICHIERS and not nom.startswith(MOTIF_CPU):
                continue
            (CIBLE / nom).write_bytes(archive.read(entree))
            pris.append(nom)

    manquants = [n for n in FICHIERS if n not in pris]
    if manquants:
        raise SystemExit(f"Fichiers absents de l'archive : {', '.join(manquants)}")
    if not any(n.startswith(MOTIF_CPU) for n in pris):
        raise SystemExit("Aucune bibliotheque de calcul processeur dans l'archive.")

    poids = sum(f.stat().st_size for f in CIBLE.iterdir())
    print(f"{len(pris)} fichiers extraits, {poids / 1e6:.1f} Mo.")


def poser_licence() -> None:
    print("Recuperation de l'avis de licence a la source...")
    with urllib.request.urlopen(URL_LICENCE, timeout=60) as reponse:
        texte = reponse.read().decode("utf-8")
    if "MIT" not in texte or "Permission is hereby granted" not in texte:
        raise SystemExit("Le texte recupere ne ressemble pas a une licence MIT.")
    entete = (
        "Ce repertoire contient des binaires de whisper.cpp (ggml-org/whisper.cpp),\n"
        f"etiquette {ETIQUETTE}, redistribues sous licence MIT.\n"
        "Cet avis accompagne obligatoirement toute copie.\n\n"
        "----------------------------------------------------------------------\n\n"
    )
    (CIBLE / NOM_LICENCE).write_text(entete + texte, encoding="utf-8")


def main() -> int:
    verifier = "--verifier" in sys.argv[1:]

    if deja_prepare():
        print(f"Moteur embarque deja en place ({CIBLE.relative_to(RACINE)}).")
        return 0

    if verifier:
        print(f"Moteur embarque ABSENT ou perime dans {CIBLE.relative_to(RACINE)}.")
        print("  Lancer : python scripts/preparer-moteur-embarque.py")
        return 1

    extraire(telecharger())
    poser_licence()
    (CIBLE / MARQUE).write_text(EMPREINTE, encoding="utf-8")

    # Verification du PRODUIT et pas de l'intention.
    if not deja_prepare():
        raise SystemExit("Le moteur prepare ne passe pas son propre controle.")
    print("Moteur embarque pret.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
