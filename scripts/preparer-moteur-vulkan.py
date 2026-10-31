"""Construit l'artefact du moteur Vulkan d’Oyant, sur cette machine.

Pourquoi ici et pas en integration continue : whisper.cpp ne publie aucune version
Vulkan toute faite, et une CI n'a pas de carte graphique, donc l'artefact ne pourrait
y etre ni mesure ni verifie. Le poste de travail est la seule machine du parc qui ait
un GPU.

Le resultat est un zip minimal, publie en copie versionnee sur dl.breizhzion.com et
epingle par son empreinte dans src-tauri/src/moteur.rs. Cette empreinte est la raison
pour laquelle l'objet distant ne doit JAMAIS etre ecrase : une installation deja faite
la verifie au demarrage.

Prerequis :
  - Vulkan SDK (glslc, vulkan-1.lib)   https://vulkan.lunarg.com/sdk/home#windows
  - CMake et les Build Tools MSVC
  - git

Usage :
  python scripts/preparer-moteur-vulkan.py            construit si besoin, puis assemble
  python scripts/preparer-moteur-vulkan.py --verifier  controle l'artefact sans rien refaire
"""

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

# L'etiquette amont. Elle doit rester celle du moteur processeur embarque
# (scripts/preparer-moteur-embarque.py) : un modele charge par une version de ggml
# et calcule par une autre n'est pas un montage qu'on souhaite avoir a diagnostiquer.
ETIQUETTE = "b5130"
DEPOT = "https://github.com/ggml-org/whisper.cpp.git"

# ⛔ **A incrementer a chaque changement du CONTENU de l'artefact, meme a etiquette egale.**
# L'objet en ligne est epingle par son empreinte dans les installations deja faites : le
# republier sous le meme nom casserait leur verification. `r2` ajoute Parakeet (2026-10-31) ;
# la premiere publication, sans suffixe, reste en ligne telle quelle.
REVISION = 2

RACINE = Path(__file__).resolve().parent.parent
TRAVAIL = RACINE / ".scratch" / "moteur-vulkan"
SOURCES = TRAVAIL / "whisper.cpp"
CONSTRUCTION = TRAVAIL / "build"
ARTEFACT = TRAVAIL / f"oyant-moteur-windows-x64-vulkan-{ETIQUETTE}-r{REVISION}.zip"

# Les programmes a construire. `parakeet-cli` lit les modeles Parakeet, que `whisper-cli` ne sait
# pas lire : sans lui, Oyant fait passer Parakeet par le moteur processeur embarque.
CIBLES = ["whisper-cli", "parakeet-cli"]

# Les seuls fichiers qui servent. L'archive amont en embarque une quarantaine d'autres
# (bancs d'essai, serveur, jeu d'echecs, binaires de test) dont aucun n'est appele.
ATTENDUS = [
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-vulkan.dll",
    "parakeet-cli.exe",
    "parakeet.dll",
]
# Plus toutes les variantes de processeur, choisies au demarrage selon la machine.
PREFIXE_CPU = "ggml-cpu-"


def executer(commande, cwd=None, env=None):
    print("  $", " ".join(str(c) for c in commande))
    resultat = subprocess.run(commande, cwd=cwd, env=env)
    if resultat.returncode != 0:
        sys.exit(f"echec ({resultat.returncode}) : {' '.join(str(c) for c in commande)}")


def trouver_sdk_vulkan():
    """Le SDK pose VULKAN_SDK dans l'environnement, mais pas toujours dans la session
    en cours juste apres son installation. On accepte les deux, on echoue clairement."""
    depuis_env = os.environ.get("VULKAN_SDK")
    if depuis_env and (Path(depuis_env) / "Bin" / "glslc.exe").exists():
        return Path(depuis_env)
    for racine in (Path("D:/VulkanSDK"), Path("C:/VulkanSDK")):
        if racine.is_dir():
            versions = sorted((d for d in racine.iterdir() if d.is_dir()), reverse=True)
            for version in versions:
                if (version / "Bin" / "glslc.exe").exists():
                    return version
    sys.exit(
        "Vulkan SDK introuvable. Installer depuis https://vulkan.lunarg.com/sdk/home#windows,\n"
        "puis relancer (ou poser VULKAN_SDK a la main)."
    )


def recuperer_sources():
    if (SOURCES / ".git").is_dir():
        revision = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=SOURCES, capture_output=True, text=True
        ).stdout.strip()
        print(f"  sources deja presentes, revision {revision[:12]}")
        return
    TRAVAIL.mkdir(parents=True, exist_ok=True)
    executer(["git", "clone", "--depth", "1", "--branch", ETIQUETTE, DEPOT, str(SOURCES)])


def construire(sdk):
    binaires = CONSTRUCTION / "bin" / "Release"
    # ⚠️ TOUTES les cibles, pas seulement la premiere : un repertoire construit avant l'ajout de
    # Parakeet a `whisper-cli.exe` et pas `parakeet-cli.exe`, et s'arreter au premier ferait
    # echouer l'assemblage sur un fichier manquant au lieu de le construire.
    if all((binaires / f"{cible}.exe").exists() for cible in CIBLES):
        print("  binaires deja construits")
        return binaires

    env = dict(os.environ)
    env["VULKAN_SDK"] = str(sdk)
    env["PATH"] = str(sdk / "Bin") + os.pathsep + env["PATH"]

    executer(
        [
            "cmake",
            "-B", str(CONSTRUCTION),
            "-DGGML_VULKAN=ON",
            # Sans ca, le binaire est taille pour le processeur de la machine de
            # construction et plante en instruction illegale ailleurs. Le defaut
            # de CMake est ON, et le defaut est le mauvais choix pour un artefact
            # qu'on distribue.
            "-DGGML_NATIVE=OFF",
            # Backends charges dynamiquement : sans ca, ggml.dll importe
            # vulkan-1.dll statiquement et le moteur refuse de demarrer sur une
            # machine sans pilote Vulkan, au lieu de retomber sur le processeur.
            "-DGGML_BACKEND_DL=ON",
            "-DGGML_CPU_ALL_VARIANTS=ON",
            "-DBUILD_SHARED_LIBS=ON",
            "-DWHISPER_BUILD_TESTS=OFF",
            "-DWHISPER_BUILD_EXAMPLES=ON",
        ],
        cwd=SOURCES,
        env=env,
    )
    executer(
        ["cmake", "--build", str(CONSTRUCTION), "--config", "Release", "--target", *CIBLES],
        cwd=SOURCES,
        env=env,
    )
    return binaires


def rassembler(binaires):
    fichiers = []
    for nom in ATTENDUS:
        chemin = binaires / nom
        if not chemin.exists():
            sys.exit(f"manquant apres construction : {chemin}")
        fichiers.append(chemin)

    variantes = sorted(binaires.glob(f"{PREFIXE_CPU}*.dll"))
    if not variantes:
        sys.exit(
            "aucune variante de processeur produite : GGML_CPU_ALL_VARIANTS n'a pas pris.\n"
            "Supprimer le repertoire de construction et relancer."
        )
    fichiers.extend(variantes)

    licence = SOURCES / "LICENSE"
    if not licence.exists():
        sys.exit(f"licence amont introuvable : {licence}")

    if ARTEFACT.exists():
        ARTEFACT.unlink()
    with zipfile.ZipFile(ARTEFACT, "w", zipfile.ZIP_DEFLATED) as archive:
        for chemin in fichiers:
            archive.write(chemin, chemin.name)
        archive.write(licence, "LICENSE-whisper.cpp.txt")
    return fichiers


def empreinte(chemin):
    condensat = hashlib.sha256()
    with open(chemin, "rb") as fichier:
        for bloc in iter(lambda: fichier.read(1024 * 1024), b""):
            condensat.update(bloc)
    return condensat.hexdigest()


def decrire():
    if not ARTEFACT.exists():
        sys.exit(f"artefact absent : {ARTEFACT}\nLancer sans --verifier pour le construire.")
    with zipfile.ZipFile(ARTEFACT) as archive:
        noms = sorted(archive.namelist())
    taille = ARTEFACT.stat().st_size
    print()
    print(f"  artefact  : {ARTEFACT}")
    print(f"  contenu   : {len(noms)} fichiers")
    for nom in noms:
        print(f"              {nom}")
    print(f"  taille    : {taille} octets ({taille / 1024 / 1024:.1f} Mo)")
    print(f"  empreinte : {empreinte(ARTEFACT)}")
    print()
    print("  A reporter dans src-tauri/src/moteur.rs (champs taille et empreinte),")
    print("  puis publier en copie versionnee, sans jamais ecraser une version deja en ligne.")


def main():
    analyseur = argparse.ArgumentParser(description=__doc__)
    analyseur.add_argument(
        "--verifier", action="store_true", help="decrit l'artefact existant sans rien reconstruire"
    )
    arguments = analyseur.parse_args()

    if arguments.verifier:
        decrire()
        return

    if shutil.which("cmake") is None:
        sys.exit("cmake introuvable dans le PATH.")
    if shutil.which("git") is None:
        sys.exit("git introuvable dans le PATH.")

    sdk = trouver_sdk_vulkan()
    print(f"Vulkan SDK : {sdk}")
    print(f"Etiquette  : {ETIQUETTE}")

    recuperer_sources()
    binaires = construire(sdk)
    rassembler(binaires)
    decrire()


if __name__ == "__main__":
    main()
