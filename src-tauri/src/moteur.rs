//! Le moteur de transcription : son installation, et son appel.
//!
//! ⛔ **Le support GPU n'est pas une question de code Rust, c'est une question de QUEL BINAIRE on
//! installe.** whisper.cpp publie un artefact par dos d'execution (processeur seul, BLAS, CUDA).
//! C'est pourquoi le moteur se telecharge comme un modele au lieu d'etre compile avec nous :
//! changer de dos d'execution devient un changement de fichier, pas un changement de programme.
//!
//! ⚠️ **Piege releve le 2026-09-17 en lisant les publications reelles : les etiquettes de VERSION
//! de whisper.cpp n'ont AUCUN binaire.** `v1.9.4` et `v1.9.3` sont vides ; les artefacts vivent
//! sur des etiquettes de compilation (`b5130`). Suivre « la derniere version » donnerait donc
//! zero fichier, avec un message qui accuserait le reseau.
//!
//! ⚠️ **Et `parakeet-cli.exe` est dans l'archive officielle Windows**, contrairement a ce que la
//! documentation du projet supposait encore. Parakeet n'a donc pas besoin d'une source tierce.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

/// Un dos d'execution disponible pour une plateforme.
///
/// ⚠️ Taille et empreinte **relevees sur l'API GitHub** le 2026-09-17, jamais estimees : GitHub
/// publie le `digest` de chaque artefact, et c'est lui qui fait autorite.
pub struct Moteur {
    pub identifiant: &'static str,
    pub nom: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub taille: u64,
    pub empreinte: &'static str,
    /// Prefixe a retirer des chemins de l'archive (`Release/` chez whisper.cpp).
    pub prefixe: &'static str,
    /// Le binaire a appeler, une fois l'archive deballee.
    pub executable: &'static str,
    /// Le binaire qui lit les modeles Parakeet, vide si cette archive n'en contient pas.
    ///
    /// ⚠️ Releve dans chaque archive le 2026-10-29 et pas suppose : les archives amont (processeur,
    /// CUDA, Linux) le portent toutes, notre artefact Vulkan non, parce qu'il ne construit que la
    /// cible `whisper-cli`. Vide veut dire « Parakeet indisponible avec ce choix », et c'est dit.
    pub executable_parakeet: &'static str,
    /// Livre PAR L'INSTALLATEUR, donc rien a telecharger ni a verifier a l'execution.
    ///
    /// ⛔ C'est le plancher du produit : Oyant doit savoir dicter des la fin de l'installation,
    /// sans reseau. Un catalogue distant rend le logiciel dependant de notre disponibilite, donc
    /// le choix par defaut ne doit jamais en dependre.
    pub embarque: bool,
    /// Le dos d'execution que ce choix doit REELLEMENT charger, vide pour le processeur seul.
    ///
    /// ⛔ Existe parce qu'une archive peut etre complete, verifiee par son empreinte, et
    /// **incapable de charger son acceleration** : whisper.cpp retombe alors sur le processeur
    /// sans un mot. Ce champ est ce qu'on va CHERCHER dans la sortie du moteur, jamais ce qu'on
    /// affirme.
    pub acceleration_attendue: &'static str,
    /// Peut-on reellement l'installer aujourd'hui ?
    ///
    /// ⚠️ Un choix indisponible est **montre quand meme, avec sa raison**, contrairement a la
    /// regle qu'on applique au menu de l'icone. La difference est ce que l'absence apprend :
    /// une entree grisee « Enregistrer » dans un menu ne repond a aucune question, alors que
    /// quelqu'un qui a une carte AMD a besoin de savoir si elle est prise en charge. Sans cette
    /// ligne, il chercherait sans fin une option qui n'existe pas.
    pub disponible: bool,
    pub indisponible_parce_que: &'static str,
    /// Bibliotheque systeme sans laquelle ce choix n'a aucun sens sur cette machine, vide s'il
    /// vaut partout.
    ///
    /// ⚠️ On teste la bibliotheque du PILOTE et pas le nom de la carte. Une carte peut porter la
    /// bonne marque sans que le pilote qui la rend calculable soit installe, et c'est justement
    /// ce cas qui produirait un choix propose, telecharge, puis silencieusement retombe sur le
    /// processeur. La presence de la bibliotheque est exactement la condition d'utilisabilite.
    pub bibliotheque_requise: &'static str,
}

#[cfg(target_os = "windows")]
pub const MOTEURS: &[Moteur] = &[
    Moteur {
        identifiant: "windows-x64-cpu",
        embarque: true,
        acceleration_attendue: "",
        nom: "Le processeur",
        // ⚠️ Ecrit pour quelqu'un qui decouvre. « Dos d'execution » etait la traduction litterale
        // de *backend* : un mot que personne ne dit et qui n'explique rien.
        description: "Fourni avec Oyant, rien à télécharger. Fonctionne sur toutes les machines. Plus lent, mais c'est le choix sûr.",
        // Vides : ce moteur est livre par l'installateur. `scripts/preparer-moteur-embarque.py`
        // porte l'etiquette amont, l'empreinte attendue et le jeu minimal de fichiers.
        url: "",
        taille: 0,
        empreinte: "",
        prefixe: "Release/",
        executable: "whisper-cli.exe",
        // ⚠️ Present seulement si `scripts/preparer-moteur-embarque.py` le garde dans son jeu
        // minimal : l'archive amont le contient, le moteur embarque n'en prend qu'une partie.
        executable_parakeet: "parakeet-cli.exe",
        disponible: true,
        indisponible_parce_que: "",
        // Le processeur est toujours la : c'est le plancher du produit.
        bibliotheque_requise: "",
    },
    Moteur {
        identifiant: "windows-x64-nvidia",
        embarque: false,
        acceleration_attendue: "CUDA",
        nom: "Une carte graphique NVIDIA",
        description: "Nettement plus rapide si vous avez une carte NVIDIA et un pilote à jour.",
        // ⛔ **CUDA 12.4 et surtout pas 11.8, alors que 11.8 est 2,5 fois plus leger.**
        // L'archive `whisper-cublas-11.8.0-bin-x64.zip` a d'abord ete retenue pour sa taille,
        // puis MESUREE le 2026-09-17 : elle livre un `ggml-cuda.dll` de 518 Mo qui importe
        // `cublas64_11.dll`, **absent de l'archive et absent de Windows**. Le dos CUDA ne se
        // charge donc jamais, whisper.cpp retombe en silence sur le processeur, et la seule
        // trace est un temps de transcription identique. Un ecran annoncant « carte graphique »
        // sur un calcul fait par le processeur est precisement le mensonge qu'on veut interdire.
        // L'archive 12.4, elle, embarque bien `cublas64_12.dll` et `cublasLt64_12.dll`.
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-cublas-12.4.0-bin-x64.zip",
        taille: 674_539_285,
        empreinte: "af520ddd034d985b55dfeea3e465ed93653ba2aee1a55e865033edc548c272a7",
        prefixe: "Release/",
        executable: "whisper-cli.exe",
        executable_parakeet: "parakeet-cli.exe",
        disponible: true,
        indisponible_parce_que: "",
        // Posee dans System32 par le pilote NVIDIA. Sans elle, ce choix ferait telecharger
        // 675 Mo pour un calcul qui repartirait sur le processeur sans le dire.
        bibliotheque_requise: "nvcuda.dll",
    },
    Moteur {
        identifiant: "windows-x64-amd-intel",
        embarque: false,
        acceleration_attendue: "Vulkan",
        nom: "N'importe quelle carte graphique",
        description: "Presque aussi rapide que l'option NVIDIA, trente fois plus léger, et fonctionne chez tous les fabricants.",
        // whisper.cpp ne publie aucun artefact Vulkan : celui-ci est **le notre**, construit par
        // `scripts/preparer-moteur-vulkan.py` sur le poste de travail, seule machine du parc qui
        // ait une carte graphique, donc seule ou il peut etre mesure. Deux options de compilation
        // font toute la difference entre un artefact distribuable et un artefact qui marche
        // uniquement sur la machine qui l'a produit, le script porte le detail.
        //
        // ⚠️ L'objet distant ne doit JAMAIS etre ecrase : l'empreinte ci-dessous est verifiee par
        // les installations deja faites. Une nouvelle version amont prend une nouvelle URL.
        url: "https://dl.breizhzion.com/oyant-desktop/moteurs/oyant-moteur-windows-x64-vulkan-b5130.zip",
        taille: 20_955_915,
        empreinte: "efdcef4dfbc912e6080d042b3aa0881fd2a7d90b7a5fa4c45f2ce22e5674c7ab",
        // A plat, contrairement aux archives amont qui rangent tout sous `Release/`.
        prefixe: "",
        executable: "whisper-cli.exe",
        // ⚠️ Vide tant que notre artefact ne construit que la cible `whisper-cli`. L'y ajouter
        // demande une NOUVELLE URL, l'objet distant actuel ne devant jamais etre ecrase.
        executable_parakeet: "",
        disponible: true,
        indisponible_parce_que: "",
        // ⚠️ Volontairement vide, alors que `vulkan-1.dll` conditionne bel et bien ce moteur.
        // L'asymetrie avec CUDA est deliberee : masquer CUDA ne prive de rien puisque Vulkan
        // reste, tandis que masquer Vulkan sur une machine dont le pilote est simplement a
        // mettre a jour retirerait la SEULE option acceleree, sans dire pourquoi. Le repli
        // processeur est propre et la sonde dit la verite, donc le cout d'une proposition de
        // trop est un telechargement de 21 Mo, pas un mensonge.
        bibliotheque_requise: "",
    },
];

// Les autres plateformes arrivent a leur etape. Une liste vide est honnete ; inventer une URL
// qui n'a jamais ete verifiee ne le serait pas.
/// ⚠️ **Linux EST publie par l'amont**, contrairement a ce que ce projet a suppose jusqu'au
/// 2026-09-20 : `whisper-bin-ubuntu-x64.tar.gz` et sa variante ARM64 vivent dans la meme
/// publication que les archives Windows. Il n'y a donc rien a construire, juste une autre
/// extension d'archive.
///
/// ⛔ **Un seul moteur par architecture, et pas de choix a proposer.** L'amont ne publie ni CUDA
/// ni Vulkan pour Linux : offrir une option acceleree qui n'existe pas serait le meme mensonge
/// que d'annoncer une carte graphique sur un calcul fait par le processeur.
#[cfg(target_os = "linux")]
pub const MOTEURS: &[Moteur] = &[Moteur {
    #[cfg(target_arch = "x86_64")]
    identifiant: "linux-x64-cpu",
    #[cfg(target_arch = "aarch64")]
    identifiant: "linux-arm64-cpu",
    embarque: false,
    acceleration_attendue: "",
    nom: "Le processeur",
    description: "Fonctionne sur toutes les machines. C'est le seul moteur publié pour Linux aujourd'hui.",
    // Taille et empreinte RELEVEES sur l'API GitHub le 2026-09-20, jamais estimees.
    #[cfg(target_arch = "x86_64")]
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-bin-ubuntu-x64.tar.gz",
    #[cfg(target_arch = "x86_64")]
    taille: 9_793_438,
    #[cfg(target_arch = "x86_64")]
    empreinte: "53e7fd8b5764edad916b8848dd0af6abb1ff1d3b86c899e79c78652412536c32",
    #[cfg(target_arch = "x86_64")]
    prefixe: "whisper-bin-ubuntu-x64/",

    #[cfg(target_arch = "aarch64")]
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-bin-ubuntu-arm64.tar.gz",
    #[cfg(target_arch = "aarch64")]
    taille: 4_605_905,
    #[cfg(target_arch = "aarch64")]
    empreinte: "93532a0e3777f26f041ffa358ee77dd88b1a33a86847c1990745327ff335a5d6",
    #[cfg(target_arch = "aarch64")]
    prefixe: "whisper-bin-ubuntu-arm64/",

    // Pas de `.exe`, et le nom differe de la version Windows.
    executable: "whisper-cli",
    executable_parakeet: "parakeet-cli",
    disponible: true,
    indisponible_parce_que: "",
    bibliotheque_requise: "",
}];

/// macOS n'a **aucun binaire publie** : l'amont ne livre qu'un `xcframework`, c'est-a-dire une
/// bibliotheque a embarquer dans une application Apple, et pas un programme a appeler. Son moteur
/// devra etre construit et heberge par nous, comme l'artefact Vulkan.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub const MOTEURS: &[Moteur] = &[];

pub fn par_identifiant(identifiant: &str) -> Option<&'static Moteur> {
    MOTEURS.iter().find(|m| m.identifiant == identifiant)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EtatMoteur {
    pub identifiant: String,
    pub nom: String,
    pub description: String,
    pub taille_attendue: u64,
    /// L'executable est en place et utilisable.
    pub installe: bool,
    /// Livre avec Oyant : rien a telecharger, et il n'y a pas de taille a annoncer.
    pub embarque: bool,
    /// C'est celui qu’Oyant utilise.
    pub actif: bool,
    /// Les dos d'execution reellement charges, mesures. Vide si rien n'est installe.
    pub acceleration_chargee: Vec<String>,
    /// L'acceleration promise par ce choix se charge-t-elle vraiment ?
    ///
    /// `None` tant que rien n'est installe, ou quand le choix ne promet aucune acceleration.
    pub acceleration_confirmee: Option<bool>,
    pub disponible: bool,
    pub indisponible_parce_que: String,
    pub chemin: String,
}

/// Le moteur retenu dans les reglages, ou le premier disponible a defaut.
///
/// ⚠️ **Si le choix enregistre n'est plus installable, on retombe sur le processeur** plutot que
/// d'echouer : quelqu'un qui change de machine emporte ses reglages dans son profil itinerant, et
/// un choix « carte NVIDIA » sur une machine sans carte NVIDIA ne doit pas empecher de dicter.
pub fn actif() -> Option<&'static Moteur> {
    let choisi = crate::reglages::lire_sans_application().calcul;
    par_identifiant(&choisi)
        .filter(|m| m.disponible)
        .or_else(|| MOTEURS.iter().find(|m| m.disponible))
}

/// Progression envoyee a l'interface pendant l'installation.
#[derive(Clone, Serialize)]
struct ProgressionMoteur {
    identifiant: String,
    recus: u64,
    total: u64,
}

/// Repertoire du moteur.
///
/// Meme regle que pour les modeles : profil LOCAL et jamais itinerant, et jamais le repertoire
/// d'installation que la desinstallation emporte.
fn dossier(moteur: &Moteur) -> Result<PathBuf, String> {
    if moteur.embarque {
        return crate::chemins::moteur_embarque();
    }
    Ok(crate::chemins::moteurs()?.join(moteur.identifiant))
}

/// Le binaire a appeler pour ce choix.
///
/// ⛔ **Ne prend PAS d'`AppHandle`, et c'est deliberé.** La ligne de commande n'en a pas, donc
/// elle recalculait le chemin de son cote : elle cherchait le moteur dans le repertoire des
/// telechargements et **ignorait la branche « embarque »**, si bien qu'elle repondait « moteur non
/// installe » sur une machine ou il etait livre avec l'application. Une seule fonction, appelee
/// par les deux.
pub fn executable(moteur: &Moteur) -> Result<PathBuf, String> {
    Ok(dossier(moteur)?.join(moteur.executable))
}

/// Le binaire qui sait lire un modele de cette famille, dans ce moteur.
///
/// ⛔ **Une archive sans Parakeet rend une ERREUR, pas le binaire de Whisper.** Donner un modele
/// Parakeet a `whisper-cli` echouerait sur un message de format de fichier, qui enverrait chercher
/// un telechargement corrompu au lieu d'un moteur qui ne sait pas le lire.
pub fn executable_pour(
    moteur: &Moteur,
    famille: crate::modeles::Famille,
) -> Result<PathBuf, String> {
    match famille {
        crate::modeles::Famille::Whisper => executable(moteur),
        crate::modeles::Famille::Parakeet => {
            if moteur.executable_parakeet.is_empty() {
                return Err(format!(
                    "« {} » ne sait pas encore faire tourner Parakeet.",
                    moteur.nom
                ));
            }
            Ok(dossier(moteur)?.join(moteur.executable_parakeet))
        }
    }
}

/// Marque portant l'empreinte de l'ARCHIVE qui a produit cette installation.
///
/// ⛔ Sans elle, « installe » voulait dire « l'executable existe », donc changer d'archive pour un
/// meme choix ne reinstallait RIEN : l'ancienne version restait en place et le reglage basculait
/// quand meme. Constate le 2026-09-17 en passant de CUDA 11.8 a 12.4, la bascule s'est faite sans
/// telecharger un octet et sans le moindre message.
fn marque(moteur: &Moteur) -> Result<PathBuf, String> {
    Ok(dossier(moteur)?.join("installe.empreinte"))
}

/// L'archive attendue est-elle celle qui est reellement installee ?
fn a_jour(moteur: &Moteur) -> bool {
    let Ok(binaire) = executable(moteur) else {
        return false;
    };
    if !binaire.is_file() {
        return false;
    }
    // Un moteur embarque est a jour par construction : il a ete livre avec l'executable.
    if moteur.embarque {
        return true;
    }
    marque(moteur)
        .and_then(|m| fs::read_to_string(m).map_err(|e| e.to_string()))
        .map(|contenu| contenu.trim() == moteur.empreinte)
        .unwrap_or(false)
}

fn empreinte_de(chemin: &Path) -> Result<String, String> {
    let mut fichier =
        fs::File::open(chemin).map_err(|erreur| format!("Lecture impossible : {erreur}"))?;
    let mut hacheur = Sha256::new();
    let mut tampon = vec![0u8; 1024 * 1024];
    loop {
        let lus = fichier
            .read(&mut tampon)
            .map_err(|erreur| format!("Lecture interrompue : {erreur}"))?;
        if lus == 0 {
            break;
        }
        hacheur.update(&tampon[..lus]);
    }
    let resume = hacheur.finalize();
    let mut hexadecimal = String::with_capacity(resume.len() * 2);
    for octet in resume.iter() {
        hexadecimal.push_str(&format!("{octet:02x}"));
    }
    Ok(hexadecimal)
}

fn etat_de(moteur: &Moteur) -> EtatMoteur {
    let binaire = executable(moteur).unwrap_or_default();
    let choisi = actif().map(|m| m.identifiant).unwrap_or("");
    let installe = moteur.disponible && a_jour(moteur);

    // ⚠️ La sonde n'est lancee que sur ce qui est installe : demarrer un processus par entree du
    // catalogue a chaque rafraichissement d'ecran serait cher pour rien.
    let charges = if installe {
        acceleration_chargee(&binaire)
    } else {
        Vec::new()
    };
    let confirmee = if !installe || moteur.acceleration_attendue.is_empty() {
        None
    } else {
        Some(
            charges
                .iter()
                .any(|dos| dos.eq_ignore_ascii_case(moteur.acceleration_attendue)),
        )
    };
    EtatMoteur {
        identifiant: moteur.identifiant.to_string(),
        nom: moteur.nom.to_string(),
        description: moteur.description.to_string(),
        taille_attendue: moteur.taille,
        installe,
        embarque: moteur.embarque,
        actif: moteur.disponible && choisi == moteur.identifiant,
        disponible: moteur.disponible,
        indisponible_parce_que: moteur.indisponible_parce_que.to_string(),
        acceleration_chargee: charges,
        acceleration_confirmee: confirmee,
        chemin: binaire.display().to_string(),
    }
}

/// Retient le choix, apres s'etre assure que le programme est bien la.
///
/// ⛔ **L'ordre compte : on installe AVANT d'enregistrer le choix.** Enregistrer d'abord
/// laisserait, si le telechargement echoue, un reglage qui designe un programme absent : Oyant
/// refuserait alors de transcrire en disant que le programme manque, sur un choix que
/// l'utilisateur vient pourtant de faire.
#[tauri::command]
pub async fn choisir_moteur(
    app: AppHandle,
    identifiant: String,
) -> Result<Vec<EtatMoteur>, String> {
    let moteur = par_identifiant(&identifiant).ok_or("Choix inconnu")?;
    if !moteur.disponible {
        return Err(moteur.indisponible_parce_que.to_string());
    }
    // ⚠️ L'ecran ne propose pas ce choix quand le pilote manque, mais la commande est appelable
    // autrement (ligne de commande, reglage recopie d'une autre machine). Refuser ici evite de
    // telecharger des centaines de megaoctets pour une acceleration qui ne se chargera pas.
    if !pertinent(moteur, &bibliotheques_presentes(), "") {
        return Err(format!(
            "Ce choix demande {}, qui n'est pas installée sur cet ordinateur.",
            moteur.bibliotheque_requise
        ));
    }

    if !a_jour(moteur) {
        installer_moteur(app.clone(), identifiant.clone()).await?;
    }

    let mut reglages = crate::reglages::lire_sans_application();
    reglages.calcul = identifiant;
    crate::reglages::ecrire_reglages(app.clone(), reglages)?;

    Ok(etat_moteurs())
}

/// Les bibliotheques de pilote presentes sur cette machine, parmi celles que le catalogue exige.
///
/// ⚠️ On regarde dans le repertoire systeme et nulle part ailleurs : c'est la que les pilotes
/// posent leur bibliotheque, et c'est le seul endroit ou Windows la chargera pour un processus
/// qui ne l'a pas dans son propre repertoire.
#[cfg(target_os = "windows")]
fn bibliotheques_presentes() -> Vec<String> {
    let systeme = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let systeme = PathBuf::from(systeme).join("System32");
    MOTEURS
        .iter()
        .map(|moteur| moteur.bibliotheque_requise)
        .filter(|nom| !nom.is_empty())
        .filter(|nom| systeme.join(nom).exists())
        .map(str::to_string)
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn bibliotheques_presentes() -> Vec<String> {
    Vec::new()
}

/// Ce choix a-t-il un sens sur une machine ou ces bibliotheques sont presentes ?
///
/// Fonction PURE, pour etre testable sans dependre du materiel de la machine qui lance les tests.
fn pertinent(moteur: &Moteur, presentes: &[String], choisi: &str) -> bool {
    // ⛔ **Le choix courant reste toujours visible, meme devenu hors sujet.** Les reglages
    // suivent l'utilisateur d'une machine a l'autre : masquer le moteur qui calcule vraiment
    // ferait afficher un ecran ou AUCUNE option n'est marquee « utilisé », et le seul moyen d'en
    // sortir serait d'en choisir un autre sans comprendre pourquoi.
    if moteur.identifiant == choisi {
        return true;
    }
    moteur.bibliotheque_requise.is_empty()
        || presentes
            .iter()
            .any(|nom| nom.eq_ignore_ascii_case(moteur.bibliotheque_requise))
}

#[tauri::command]
pub fn etat_moteurs() -> Vec<EtatMoteur> {
    let presentes = bibliotheques_presentes();
    let choisi = crate::reglages::lire_sans_application().calcul;
    MOTEURS
        .iter()
        .filter(|moteur| pertinent(moteur, &presentes, &choisi))
        .map(etat_de)
        .collect()
}

/// Telecharge l'archive, verifie son empreinte, puis la deballe.
///
/// ⚠️ **Pas de reprise ici, contrairement aux modeles, et c'est delibere.** Une archive de 8 Mo
/// qui echoue se relance en quelques secondes ; y porter la machinerie de reprise ajouterait du
/// code a maintenir pour un gain nul. Les 3 Go d'un modele, eux, la justifient.
///
/// ⚠️ **L'empreinte est verifiee AVANT de deballer.** Deballer d'abord repandrait des fichiers
/// d'un archive corrompue dans le repertoire du moteur, ou ils se feraient passer pour une
/// installation valide.
#[tauri::command]
pub async fn installer_moteur(app: AppHandle, identifiant: String) -> Result<EtatMoteur, String> {
    let moteur = par_identifiant(&identifiant).ok_or("Moteur inconnu")?;
    if moteur.embarque {
        return Err("Ce moteur est fourni avec Oyant, il n'y a rien à installer.".to_string());
    }
    let cible = dossier(moteur)?;
    fs::create_dir_all(&cible).map_err(|erreur| format!("Répertoire non créé : {erreur}"))?;

    let app_tache = app.clone();
    let id = identifiant.clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let moteur = par_identifiant(&id).unwrap();
        let cible = dossier(moteur)?;
        // ⚠️ L'archive est deposee dans le PARENT, pas dans le repertoire cible : celui-ci est
        // vide juste avant l'extraction, et une archive posee dedans s'effacerait elle-meme.
        let archive = cible
            .parent()
            .ok_or("Répertoire parent introuvable")?
            .join(format!("{}.partiel", moteur.identifiant));

        let reponse = ureq::get(moteur.url)
            .call()
            .map_err(|erreur| format!("Téléchargement impossible : {erreur}"))?;

        let mut flux = reponse.into_reader();
        let mut sortie =
            fs::File::create(&archive).map_err(|erreur| format!("Fichier non créé : {erreur}"))?;

        // ⚠️ Recopie par blocs plutot qu'un `io::copy` d'un trait : 260 Mo sans le moindre signe
        // de vie feraient croire l'application bloquee, et on la fermerait.
        let mut tampon = vec![0u8; 256 * 1024];
        let mut recus: u64 = 0;
        let mut dernier = std::time::Instant::now();
        loop {
            let lus = flux
                .read(&mut tampon)
                .map_err(|erreur| format!("Transfert interrompu : {erreur}"))?;
            if lus == 0 {
                break;
            }
            std::io::Write::write_all(&mut sortie, &tampon[..lus])
                .map_err(|erreur| format!("Écriture impossible : {erreur}"))?;
            recus += lus as u64;
            if dernier.elapsed().as_millis() >= 100 {
                let _ = app_tache.emit(
                    "moteur-progression",
                    ProgressionMoteur {
                        identifiant: id.clone(),
                        recus,
                        total: moteur.taille,
                    },
                );
                dernier = std::time::Instant::now();
            }
        }
        drop(sortie);

        let calcule = empreinte_de(&archive)?;
        if calcule != moteur.empreinte {
            let _ = fs::remove_file(&archive);
            return Err(
                "L'archive du moteur est abîmée : son empreinte ne correspond pas. \
                 Rien n'a été installé, vous pouvez relancer."
                    .to_string(),
            );
        }

        // ⛔ **On vide le repertoire cible avant de deballer.** Sans ca, changer d'archive pour
        // un meme choix (c'est arrive le 2026-09-17 en passant de CUDA 11.8 a 12.4) laisse les
        // fichiers de l'ancienne a cote de la nouvelle : des bibliotheques de deux versions
        // cohabitent, et celle qui se charge depend de l'ordre de recherche de Windows.
        if cible.exists() {
            fs::remove_dir_all(&cible)
                .map_err(|erreur| format!("Ancienne version non retirée : {erreur}"))?;
        }
        fs::create_dir_all(&cible).map_err(|erreur| format!("Répertoire non créé : {erreur}"))?;

        deballer(&archive, &cible, moteur.prefixe)?;
        let _ = fs::remove_file(&archive);

        // ⚠️ La marque est ecrite EN DERNIER : si l'extraction echoue a mi-chemin, l'installation
        // ne se declare pas a jour et sera refaite, plutot que de rester a moitie posee.
        fs::write(marque(moteur)?, moteur.empreinte)
            .map_err(|erreur| format!("Marque non écrite : {erreur}"))?;
        Ok(())
    })
    .await
    .map_err(|erreur| format!("Installation interrompue : {erreur}"))??;

    Ok(etat_de(moteur))
}

/// Deballe l'archive dans le repertoire du moteur.
///
/// ⚠️ **On extrait TOUT le contenu du prefixe, sans trier.** whisper.cpp livre une dizaine de
/// bibliotheques de calcul dont il choisit la bonne a l'execution selon le processeur : en garder
/// une sous-selection donnerait un moteur qui marche sur la machine de developpement et echoue
/// ailleurs, avec un message de chargement de bibliotheque qui ne dirait pas lequel manque. Une
/// trentaine de mega-octets a cote de 3 Go de modeles ne se discute pas.
///
/// ⛔ **Chaque chemin de l'archive est verifie avant ecriture.** Une archive peut contenir des
/// chemins comme `../../ailleurs` : sans ce controle, deballer ecrirait n'importe ou sur le
/// disque. Le fichier vient d'un tiers, il se traite comme une entree non fiable meme quand son
/// empreinte est conforme, puisque l'empreinte prouve l'origine et pas l'innocuite.
/// Le format d'une archive, decide par son NOM et non par la plateforme.
///
/// ⚠️ Decider sur l'extension plutot que sur `cfg!(windows)` garde le code testable partout : le
/// deballage d'un `tar.gz` se verifie depuis Windows, alors qu'une branche conditionnelle a la
/// plateforme ne serait compilee que sur la machine qui ne peut pas la tester.
fn est_tar_gz(archive: &Path) -> bool {
    archive
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(".tar.gz")
}

fn deballer(archive: &Path, cible: &Path, prefixe: &str) -> Result<(), String> {
    if est_tar_gz(archive) {
        return deballer_tar(archive, cible, prefixe);
    }
    deballer_zip(archive, cible, prefixe)
}

/// Deballe une archive `tar.gz`, celle que l'amont publie pour Linux.
///
/// ⛔ **Un tar n'est pas un zip, et trois differences comptent.**
///
/// 1. Il porte les **droits** : sans eux, `whisper-cli` arrive sans son bit executable et Linux
///    refuse de le lancer, avec un message qui parle de permissions et pas d'installation.
/// 2. Il contient des **liens symboliques** (`libwhisper.so` vers `libwhisper.so.1.9.4`), qu'il
///    faut recreer sous peine de bibliotheques introuvables a l'execution.
/// 3. Il peut viser **hors** du repertoire cible, exactement comme un zip. Les memes garde-fous
///    s'appliquent donc, et ils sont ecrits ici plutot que delegues : `unpack` en pose de son
///    cote, mais s'en remettre a la bibliotheque rendrait la protection invisible a la relecture.
fn deballer_tar(archive: &Path, cible: &Path, prefixe: &str) -> Result<(), String> {
    let fichier =
        fs::File::open(archive).map_err(|erreur| format!("Archive illisible : {erreur}"))?;
    let flux = flate2::read::GzDecoder::new(fichier);
    let mut tar = tar::Archive::new(flux);
    // Sans cela, les droits du tar sont ignores et l'executable arrive non executable.
    tar.set_preserve_permissions(true);

    let cible = cible
        .canonicalize()
        .map_err(|erreur| format!("Répertoire cible introuvable : {erreur}"))?;

    for entree in tar
        .entries()
        .map_err(|erreur| format!("Archive invalide : {erreur}"))?
    {
        let mut entree = entree.map_err(|erreur| format!("Entrée illisible : {erreur}"))?;
        let chemin = entree
            .path()
            .map_err(|erreur| format!("Chemin illisible : {erreur}"))?
            .into_owned();

        // ⛔ Un chemin absolu ou remontant echappe au repertoire : on refuse au lieu de nettoyer,
        // parce qu'une archive qui contient ca n'est pas une archive qu'on veut deballer.
        if chemin.is_absolute()
            || chemin
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(format!(
                "L'archive contient un chemin dangereux : {}",
                chemin.display()
            ));
        }

        let Ok(relatif) = chemin.strip_prefix(prefixe) else {
            continue;
        };
        if relatif.as_os_str().is_empty() {
            continue;
        }

        let destination = cible.join(relatif);
        // Second controle apres construction, comme pour le zip.
        if !destination.starts_with(&cible) {
            return Err(format!(
                "Chemin hors du répertoire cible : {}",
                chemin.display()
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|erreur| format!("Répertoire non créé : {erreur}"))?;
        }

        entree
            .unpack(&destination)
            .map_err(|erreur| format!("Extraction interrompue : {erreur}"))?;
    }

    Ok(())
}

fn deballer_zip(archive: &Path, cible: &Path, prefixe: &str) -> Result<(), String> {
    let fichier =
        fs::File::open(archive).map_err(|erreur| format!("Archive illisible : {erreur}"))?;
    let mut zip =
        zip::ZipArchive::new(fichier).map_err(|erreur| format!("Archive invalide : {erreur}"))?;

    let cible = cible
        .canonicalize()
        .map_err(|erreur| format!("Répertoire cible introuvable : {erreur}"))?;

    for index in 0..zip.len() {
        let mut entree = zip
            .by_index(index)
            .map_err(|erreur| format!("Entrée illisible : {erreur}"))?;

        let Some(nom) = entree.enclosed_name() else {
            // `enclosed_name` rend `None` sur un chemin qui sortirait du repertoire.
            return Err(format!(
                "L'archive contient un chemin dangereux : {}",
                entree.name()
            ));
        };
        let Ok(relatif) = nom.strip_prefix(prefixe) else {
            continue;
        };
        if entree.is_dir() {
            continue;
        }

        let destination = cible.join(relatif);
        // Second controle, apres construction : une entree peut etre acceptable isolement et
        // sortir quand meme du repertoire une fois jointe.
        if !destination.starts_with(&cible) {
            return Err(format!(
                "Chemin hors du répertoire cible : {}",
                entree.name()
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|erreur| format!("Répertoire non créé : {erreur}"))?;
        }

        let mut sortie = fs::File::create(&destination)
            .map_err(|erreur| format!("Écriture impossible : {erreur}"))?;
        std::io::copy(&mut entree, &mut sortie)
            .map_err(|erreur| format!("Extraction interrompue : {erreur}"))?;
    }

    Ok(())
}

/// Les dos d'execution que le moteur charge REELLEMENT, lus dans sa propre sortie.
///
/// ⛔ **Mesure et jamais declaration.** whisper.cpp annonce chaque dos charge par une ligne
/// `load_backend: loaded X backend from ...` sur sa sortie d'erreur. C'est la seule source qui
/// dise la verite : une archive peut passer sa verification d'empreinte, etre complete au sens
/// des fichiers presents, et **ne pas pouvoir charger son acceleration** parce qu'une
/// bibliotheque dont elle depend manque. whisper.cpp retombe alors sur le processeur sans un
/// mot, et le seul symptome est un temps de transcription qui ne descend pas.
///
/// On appelle `--help`, qui charge les dos d'execution sans avoir besoin d'un modele ni d'audio.
pub fn acceleration_chargee(executable: &Path) -> Vec<String> {
    let Ok(sortie) = commande(executable).arg("--help").output() else {
        return Vec::new();
    };

    // Les lignes de chargement partent sur la sortie d'erreur, pas sur la sortie standard.
    let texte = format!(
        "{}{}",
        String::from_utf8_lossy(&sortie.stderr),
        String::from_utf8_lossy(&sortie.stdout)
    );

    annonces(&texte)
}

/// Construit l'appel d'un moteur, avec de quoi trouver le runtime Visual C++.
///
/// ⛔ **Sans ca, le moteur ne demarre pas sur un Windows sans redistribuable Visual C++.**
/// `whisper-cli.exe` importe `msvcp140.dll` et `vcomp140.dll`, qu'aucune archive amont
/// n'embarque et que Windows ne fournit pas. L'installateur les depose a cote de `oyant.exe`
/// (`scripts/preparer-runtime-vcpp.py`), mais Windows cherche dans le repertoire de
/// L'EXECUTABLE LANCE, ici celui du moteur, et pas dans le notre. Le `PATH` de l'enfant est ce
/// qui relie les deux, et il vaut aussi pour les moteurs qu'on ne compile pas, comme CUDA.
fn commande(executable: &Path) -> Command {
    let Ok(exe) = std::env::current_exe() else {
        return Command::new(executable);
    };
    let Some(repertoire) = exe.parent() else {
        return Command::new(executable);
    };
    let actuel = std::env::var("PATH").unwrap_or_default();
    commande_avec(executable, &actuel, repertoire)
}

/// Le cablage, separe de la lecture de l'environnement pour etre testable.
///
/// ⚠️ Prendre le `PATH` en parametre n'est pas de la ceremonie : sous `cargo test`, le
/// repertoire de l'executable est DEJA dans le `PATH`, donc un test qui lirait l'environnement
/// reel passerait sans que ce code y soit pour quoi que ce soit.
fn commande_avec(executable: &Path, chemin_actuel: &str, repertoire: &Path) -> Command {
    let mut commande = Command::new(executable);
    commande.env("PATH", chemin_enrichi(chemin_actuel, repertoire));

    // ⚠️ **Mesure le 2026-09-20 sur une vraie machine Debian : ceci ne sert PAS aujourd'hui.**
    // L'archive amont pose `RUNPATH` a `$ORIGIN` sur `whisper-cli`, donc il trouve ses `.so`
    // poses a cote de lui sans qu'on ait rien a dire. Je l'avais ajoute sur la supposition
    // inverse, et la verification l'a dementie.
    //
    // On le garde quand meme, et pour une raison precise : le jour ou l'on construira un
    // artefact soi-meme, comme pour Vulkan ou macOS, rien ne garantit qu'on pensera au `RUNPATH`.
    // Le symptome serait alors un moteur qui ne demarre pas en parlant de bibliotheques, loin de
    // sa cause. C'est un filet, et il est ecrit ici comme tel plutot que presente en necessite.
    #[cfg(target_os = "linux")]
    if let Some(dossier_moteur) = executable.parent() {
        let actuel = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        commande.env(
            "LD_LIBRARY_PATH",
            chemin_enrichi_avec(&actuel, dossier_moteur, ':'),
        );
    }

    commande
}

/// Place `repertoire` en tete du `PATH`, sans l'y ajouter deux fois.
///
/// Fonction PURE : le comportement se teste sans lancer de processus ni toucher a
/// l'environnement du processus courant.
fn chemin_enrichi(actuel: &str, repertoire: &Path) -> String {
    chemin_enrichi_avec(actuel, repertoire, ';')
}

/// ⚠️ Le separateur n'est pas le meme partout : `;` sous Windows, `:` ailleurs. Le passer en
/// parametre garde la fonction testable des deux facons depuis n'importe quelle machine.
fn chemin_enrichi_avec(actuel: &str, repertoire: &Path, separateur: char) -> String {
    let notre = repertoire.display().to_string();
    // ⚠️ Comparaison insensible a la casse : sur Windows deux ecritures du meme chemin
    // designent le meme repertoire, et l'ajouter a chaque appel allongerait le PATH sans fin.
    if actuel.split(separateur).any(|element| {
        element
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(notre.trim_end_matches('\\'))
    }) {
        return actuel.to_string();
    }
    if actuel.is_empty() {
        return notre;
    }
    format!("{notre}{separateur}{actuel}")
}

/// Extrait les noms de dos d'execution annonces dans une sortie de moteur.
///
/// Fonction PURE, separee de l'execution pour etre testable sans lancer de processus.
fn annonces(texte: &str) -> Vec<String> {
    // ⛔ **Deux formes d'annonce, et s'en tenir a une seule est un garde-fou qui MENT.**
    // whisper.cpp a change de format entre ses versions : `b5130` ecrit
    // `load_backend: loaded CUDA backend from ...`, alors qu'une version Vulkan de la branche 1.8
    // ecrit `ggml_vulkan: Found 1 Vulkan devices:`. La sonde ne voyait que la premiere, donc elle
    // annoncait « ne demarre pas » sur un moteur Vulkan parfaitement fonctionnel. Constate le
    // 2026-09-17 en mesurant Vulkan. Ce format peut changer encore : on en reconnait plusieurs.
    let mut trouves: Vec<String> = Vec::new();
    for ligne in texte.lines().map(str::trim) {
        if let Some(reste) = ligne.strip_prefix("load_backend: loaded ") {
            if let Some(nom) = reste.split_whitespace().next() {
                // Majuscules des deux cotes : whisper ecrit « CUDA » mais « Vulkan », et notre
                // artefact Vulkan emet les DEUX formes d'annonce a la fois. Sans cette
                // normalisation, l'ecran de mesure affiche le meme dos deux fois, orthographie
                // differemment. Mesure sur l'artefact le 2026-09-17.
                trouves.push(nom.to_uppercase());
            }
        } else if let Some(reste) = ligne.strip_prefix("ggml_") {
            // `ggml_vulkan: ...`, `ggml_cuda_init: ...` : le nom du dos precede le deux-points.
            if let Some(avant) = reste.split(':').next() {
                let nom = avant.split('_').next().unwrap_or(avant);
                if !nom.is_empty() {
                    trouves.push(nom.to_uppercase());
                }
            }
        }
    }

    // ⚠️ `dedup` seul ne retire que les doublons CONSECUTIFS : deux annonces du meme dos separees
    // par une autre ligne passaient toutes les deux. On deduplique sur l'ensemble, en gardant
    // l'ordre d'apparition, qui est celui du chargement.
    let mut vus: Vec<String> = Vec::new();
    trouves.retain(|nom| {
        if vus.contains(nom) {
            false
        } else {
            vus.push(nom.clone());
            true
        }
    });
    trouves
}

/// Ecrit un WAV de silence, 16 kHz mono, de la duree demandee.
///
/// ⚠️ Le contenu n'a aucune importance : whisper encode la fenetre audio quoi qu'elle contienne,
/// donc du silence exerce exactement les memes noyaux de calcul qu'une phrase. Ce qu'on veut,
/// c'est declencher leur compilation, pas obtenir du texte.
fn wav_de_silence(chemin: &Path, millisecondes: u32) -> Result<(), String> {
    const TAUX: u32 = 16_000;
    let echantillons = TAUX * millisecondes / 1000;
    let octets_donnees = echantillons * 2;

    let mut fichier = Vec::with_capacity(44 + octets_donnees as usize);
    fichier.extend_from_slice(b"RIFF");
    fichier.extend_from_slice(&(36 + octets_donnees).to_le_bytes());
    fichier.extend_from_slice(b"WAVEfmt ");
    fichier.extend_from_slice(&16u32.to_le_bytes()); // taille du bloc de format
    fichier.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fichier.extend_from_slice(&1u16.to_le_bytes()); // mono
    fichier.extend_from_slice(&TAUX.to_le_bytes());
    fichier.extend_from_slice(&(TAUX * 2).to_le_bytes()); // octets par seconde
    fichier.extend_from_slice(&2u16.to_le_bytes()); // alignement de bloc
    fichier.extend_from_slice(&16u16.to_le_bytes()); // bits par echantillon
    fichier.extend_from_slice(b"data");
    fichier.extend_from_slice(&octets_donnees.to_le_bytes());
    fichier.resize(44 + octets_donnees as usize, 0);

    fs::write(chemin, &fichier).map_err(|erreur| format!("Fichier d'essai non écrit : {erreur}"))
}

/// Fait compiler ses noyaux a l'acceleration, MAINTENANT plutot qu'a la premiere dictee.
///
/// ⛔ **Mesure du 2026-09-17 : le premier appel apres installation de CUDA a pris 17,7 s, les
/// suivants 1,8 s.** CUDA compile ses noyaux pour la carte au premier usage et les garde dans son
/// propre cache. Le cout est paye une fois par machine, mais sans ce prechauffage il tombe sur la
/// **premiere dictee**, celle qui donne son impression du produit. On le deplace la ou l'utilisateur
/// attend deja : juste apres un telechargement de plusieurs centaines de mega-octets.
///
/// ⚠️ Sans modele telecharge, il n'y a rien a faire tourner : on ne se plaint pas, on renvoie
/// simplement `false`. Le prechauffage est une optimisation, jamais une condition.
#[tauri::command]
pub async fn prechauffer(identifiant: String) -> Result<bool, String> {
    let moteur = par_identifiant(&identifiant).ok_or("Choix inconnu")?;
    if moteur.acceleration_attendue.is_empty() || !a_jour(moteur) {
        return Ok(false);
    }

    // Le plus petit modele present suffit : on exerce les noyaux, on ne transcrit rien d'utile.
    let dossier_modeles = crate::chemins::modeles()?;
    // ⚠️ Whisper seulement : `whisper-cli` ne sait pas lire un modele Parakeet, et un echec ici
    // serait silencieux, puisque le resultat du prechauffage est jete.
    let Some(modele) = crate::modeles::CATALOGUE
        .iter()
        .filter(|m| m.famille == crate::modeles::Famille::Whisper)
        .map(|m| dossier_modeles.join(m.fichier))
        .find(|chemin| chemin.is_file())
    else {
        return Ok(false);
    };

    let binaire = executable(moteur)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
        let essai = std::env::temp_dir().join("oyant-prechauffage.wav");
        wav_de_silence(&essai, 500)?;
        // Le resultat est jete : seul l'effet de bord sur le cache de noyaux nous interesse.
        let _ = commande(&binaire)
            .arg("-m")
            .arg(&modele)
            .arg("-f")
            .arg(&essai)
            .arg("--no-timestamps")
            .arg("--no-prints")
            .output();
        let _ = fs::remove_file(&essai);
        Ok(true)
    })
    .await
    .map_err(|erreur| format!("Préchauffage interrompu : {erreur}"))?
}

/// Ce que rend une transcription.
#[derive(Debug, Clone, Serialize)]
pub struct Transcription {
    pub texte: String,
    pub secondes: f64,
}

/// Lance le moteur sur un fichier audio.
///
/// ⚠️ **Le texte est lu sur la SORTIE STANDARD, les diagnostics sur la sortie d'erreur.** Le
/// moteur ecrit `load_backend:` et `read_audio_data:` sur stderr : les melanger collerait ces
/// lignes dans le texte dicte, qui serait alors tape dans l'application de l'utilisateur.
/// Ce qu'on demande au moteur, independamment du programme qui le fera.
pub struct Demande<'a> {
    pub famille: crate::modeles::Famille,
    pub modele: &'a Path,
    pub audio: &'a Path,
    pub langue: &'a str,
    pub fils: u32,
    // ⚠️ 0 garde le moteur au plus pres de ce qu'il a entendu. Au-dela il s'autorise a deviner
    // quand il hesite, ce qui rend un texte plus fluide et moins fidele.
    pub temperature: f32,
    // Vocabulaire de l'utilisateur, deja mis en forme par `prompt_de_vocabulaire`. `None` quand
    // il n'y en a pas : on ne passe alors PAS de `--prompt` du tout, plutot qu'un prompt vide,
    // qui compterait quand meme comme contexte.
    pub prompt: Option<&'a str>,
}

/// Les arguments a passer au moteur.
///
/// Fonction PURE : elle se teste sans moteur installe, et c'est la que se joue la difference
/// entre les deux familles.
pub fn arguments(demande: &Demande) -> Vec<std::ffi::OsString> {
    let mut arguments: Vec<std::ffi::OsString> = vec![
        "-m".into(),
        demande.modele.into(),
        "-f".into(),
        demande.audio.into(),
        "-t".into(),
        demande.fils.to_string().into(),
    ];

    match demande.famille {
        crate::modeles::Famille::Parakeet => {
            // ⛔ **Ni langue, ni temperature, ni vocabulaire** : `parakeet-cli` n'a aucune de ces
            // options et refuserait de demarrer. Il detecte seul la langue parmi les siennes,
            // ce que `Famille::connait` a verifie avant d'arriver ici.
            arguments.push("--no-prints".into());
        }
        crate::modeles::Famille::Whisper => {
            arguments.extend([
                "-l".into(),
                demande.langue.into(),
                "--temperature".into(),
                demande.temperature.to_string().into(),
                // Sans horodatage : on veut du texte a taper, pas un fichier de sous-titres.
                "--no-timestamps".into(),
                "--no-prints".into(),
            ]);

            if let Some(vocabulaire) = demande.prompt {
                // ⛔ `--carry-initial-prompt` n'est pas optionnel ici. Sans lui, le prompt ne vaut
                // que pour la PREMIERE fenetre de trente secondes : une dictee plus longue
                // perdrait le vocabulaire en cours de route, et le defaut se manifesterait
                // uniquement sur les longues dictees, donc rarement et sans rapport apparent avec
                // la longueur.
                arguments.extend([
                    "--prompt".into(),
                    vocabulaire.into(),
                    "--carry-initial-prompt".into(),
                ]);
            }
        }
    }

    arguments
}

pub fn transcrire(executable: &Path, demande: &Demande) -> Result<Transcription, String> {
    if !executable.is_file() {
        return Err("The transcription engine is not installed.".to_string());
    }
    if !demande.modele.is_file() {
        return Err("The selected model is not downloaded.".to_string());
    }
    if !demande.audio.is_file() {
        return Err(format!("File not found: {}", demande.audio.display()));
    }
    // Dernier filet : `dictee::resoudre` refuse deja ce cas, avec un message dans la langue de
    // l'appelant. Celui-ci ne sert que si un futur appelant passait a cote.
    if !demande.famille.connait(demande.langue) {
        return Err(format!(
            "This model cannot transcribe language \"{}\".",
            demande.langue
        ));
    }

    let debut = std::time::Instant::now();
    let mut commande = commande(executable);
    commande.args(arguments(demande));

    let sortie = commande
        .output()
        .map_err(|erreur| format!("Engine could not be started: {erreur}"))?;

    if !sortie.status.success() {
        let details = String::from_utf8_lossy(&sortie.stderr);
        let derniere = details.lines().last().unwrap_or("no details").trim();
        return Err(format!("Engine failed: {derniere}"));
    }

    Ok(Transcription {
        texte: nettoyer(&String::from_utf8_lossy(&sortie.stdout)),
        secondes: debut.elapsed().as_secs_f64(),
    })
}

/// Plafond du prompt de vocabulaire, en caracteres.
///
/// ⚠️ **C'est un substitut, et il est volontairement prudent.** Le moteur compte en JETONS, pas en
/// caracteres, et on ne peut pas les compter ici sans embarquer son tokeniseur. La pratique
/// documentee est qu'au-dela d'environ 200 jetons le prompt degrade la transcription au lieu de
/// l'aider, en disputant le budget de contexte au texte a transcrire. A environ quatre caracteres
/// par jeton en francais, 600 caracteres restent sous cette barre avec de la marge.
const PLAFOND_PROMPT: usize = 600;

/// Construit le prompt de vocabulaire a donner au moteur, ou `None` s'il n'y a rien a donner.
///
/// Le principe, repris des projets qui le font le mieux : plutot que de corriger le texte APRES,
/// on donne au moteur les mots qu'il risque de mal entendre AVANT, pour qu'il se trompe moins.
/// C'est un **biais souple** et non une contrainte : si l'acoustique dit autre chose, le moteur
/// ecrit autre chose. Rien ne peut donc etre remplace a tort, contrairement a une correction
/// appliquee apres coup.
///
/// ⛔ **La troncature se fait a la frontiere d'une entree, jamais au milieu d'un mot.** Un prompt
/// coupe sur « Kowal » biaiserait le moteur vers un fragment qui n'existe pas, ce qui est pire que
/// de ne pas donner l'entree du tout.
///
/// ⚠️ **Verifie le 2026-09-21 : le prompt ne FUIT pas dans la sortie**, meme sur du quasi-silence,
/// ou le moteur hallucine le plus. C'etait le risque a lever avant d'ecrire cette fonction : voir
/// la liste de ses patients apparaitre dans un compte rendu serait un defaut inacceptable. Le
/// prompt change en revanche CE QUI est hallucine sur du non-parle, d'ou l'importance du seuil de
/// silence qui empeche d'envoyer au moteur un enregistrement sans parole.
pub fn prompt_de_vocabulaire(entrees: &[String]) -> Option<String> {
    let mut gardees: Vec<&str> = Vec::new();
    let mut longueur = 0;

    for entree in entrees {
        let terme = entree.trim();
        if terme.is_empty() {
            continue;
        }
        // 2 caracteres pour le separateur « , », compte seulement a partir de la deuxieme entree.
        let cout = terme.chars().count() + if gardees.is_empty() { 0 } else { 2 };
        if longueur + cout > PLAFOND_PROMPT {
            break;
        }
        longueur += cout;
        gardees.push(terme);
    }

    if gardees.is_empty() {
        return None;
    }
    Some(gardees.join(", "))
}

/// Nombre de mots au-dela duquel une cible de substitution n'est plus un terme de vocabulaire.
const MOTS_MAX_TERME: usize = 3;

/// Longueur au-dela de laquelle une cible de substitution n'est plus un terme de vocabulaire.
const LONGUEUR_MAX_TERME: usize = 40;

/// Recolte, parmi les substitutions de l'utilisateur, celles qui sont aussi du vocabulaire.
///
/// ⛔ **Le signal utile n'est PAS la sortie du moteur, et c'est tout l'enjeu de cette fonction.**
/// Collecter le vocabulaire depuis le texte transcrit serait circulaire : la sortie ne contient
/// que ce que le moteur a deja su ecrire, alors que les termes qui ont besoin d'aide sont
/// exactement ceux qui n'y apparaissent jamais. On renforcerait donc precisement le cas qui n'en
/// a pas besoin.
///
/// La cible d'une substitution, elle, est non circulaire : c'est un mot que l'utilisateur a du
/// corriger a la main, donc un mot que le moteur s'est trompe a ecrire. C'est la meilleure liste
/// de ses erreurs dont on dispose, et elle est deja dans les reglages.
///
/// ⚠️ **Le filtre est volontairement prudent, et l'asymetrie des echecs le justifie.** Oublier un
/// terme ne coute RIEN : la substitution continue de corriger le texte apres coup, comme avant.
/// Retenir a tort une tournure de ponctuation, en revanche, mange le plafond du prompt et biaise
/// le moteur vers une expression que personne n'a prononcee. Entre les deux, on rate.
///
/// D'ou les trois criteres : au plus [`MOTS_MAX_TERME`] mots, au plus [`LONGUEUR_MAX_TERME`]
/// caracteres, et **au moins une majuscule**, qui est ce qui distingue un nom propre, un acronyme
/// ou un nom de marque d'une correction de tournure comme « n'est-ce pas ? ».
pub fn termes_des_substitutions(substitutions: &[crate::texte::Substitution]) -> Vec<String> {
    substitutions
        .iter()
        .map(|substitution| substitution.remplace.trim())
        .filter(|terme| !terme.is_empty())
        .filter(|terme| terme.chars().count() <= LONGUEUR_MAX_TERME)
        .filter(|terme| terme.split_whitespace().count() <= MOTS_MAX_TERME)
        .filter(|terme| terme.chars().any(char::is_uppercase))
        .map(str::to_string)
        .collect()
}

/// Le prompt de vocabulaire a donner au moteur pour ces reglages, ou `None` s'il n'y a rien.
///
/// ⛔ **C'est le SEUL point d'entree, et c'est voulu.** La fusion entre le vocabulaire saisi et
/// celui deduit des substitutions ne doit pas vivre chez les appelants : il y en a deja deux, la
/// dictee et la ligne de commande, et un troisieme oublierait la moitie du vocabulaire sans que
/// rien ne vire au rouge. Meme famille que le repli branche ecran par ecran : pose dans la
/// fonction partagee, il ne peut plus etre oublie par personne.
///
/// ⚠️ **L'ordre decide de ce qui survit a la troncature.** Le vocabulaire saisi passe en premier
/// parce que l'utilisateur l'a ecrit deliberement ; les termes deduits comblent ce qui reste sous
/// le plafond. L'inverse ferait tomber une liste choisie a la main au profit d'un sous-produit.
pub fn prompt_des_reglages(reglages: &crate::reglages::Reglages) -> Option<String> {
    let mut entrees: Vec<String> = Vec::new();
    let mut vues: std::collections::HashSet<String> = std::collections::HashSet::new();

    for terme in reglages
        .vocabulaire
        .iter()
        .cloned()
        .chain(termes_des_substitutions(&reglages.substitutions))
    {
        let propre = terme.trim();
        if propre.is_empty() {
            continue;
        }
        // Sans casse : « ECG » saisi a la main et « ECG » deduit d'une substitution sont le meme
        // terme, et le donner deux fois au moteur gaspillerait le plafond.
        if vues.insert(propre.to_lowercase()) {
            entrees.push(propre.to_string());
        }
    }

    prompt_de_vocabulaire(&entrees)
}

/// Met la sortie du moteur en forme de texte dicte.
///
/// ⚠️ Le moteur rend un segment par ligne avec une espace de tete. Recolle tel quel, le texte
/// arriverait en colonne dans l'application visee.
pub fn nettoyer(brut: &str) -> String {
    brut.lines()
        .map(str::trim)
        .filter(|ligne| !ligne.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_segments_sont_recolles_en_une_seule_phrase() {
        let brut = "  And so my fellow Americans,\n  ask not what your country can do for you.\n\n";
        assert_eq!(
            nettoyer(brut),
            "And so my fellow Americans, ask not what your country can do for you."
        );
    }

    #[test]
    fn une_sortie_vide_rend_une_chaine_vide_et_pas_des_espaces() {
        assert_eq!(nettoyer("\n\n   \n"), "");
    }

    /// ⛔ **Garde la regression du 2026-09-17 : la sonde ne connaissait qu'un seul format
    /// d'annonce.** whisper.cpp en a au moins deux selon sa version, et n'en reconnaitre qu'un
    /// faisait annoncer « ne demarre pas » sur un moteur Vulkan parfaitement fonctionnel. Un
    /// garde-fou qui ment fait plus de degats que pas de garde-fou du tout.
    #[test]
    fn les_deux_formes_d_annonce_sont_reconnues() {
        // Forme de `b5130`, avec chargement dynamique des dos d'execution.
        let recente = "load_backend: loaded CUDA backend from C:\\x\\ggml-cuda.dll\n\
                       load_backend: loaded CPU backend from C:\\x\\ggml-cpu-haswell.dll\n";
        assert_eq!(annonces(recente), vec!["CUDA", "CPU"]);

        // Forme d'une version Vulkan de la branche 1.8, sans `load_backend` du tout.
        let vulkan = "ggml_vulkan: Found 1 Vulkan devices:\n\
                      ggml_vulkan: 0 = NVIDIA GeForce GTX 1060 6GB (NVIDIA) | uma: 0\n";
        assert_eq!(annonces(vulkan), vec!["VULKAN"]);

        // Une sortie qui n'annonce rien doit rendre une liste vide, pas inventer un dos.
        assert!(annonces("usage: whisper-cli [options]\n").is_empty());
    }

    /// ⛔ **Sortie reelle de NOTRE artefact Vulkan, relevee le 2026-09-17 sur une GTX 1060.**
    /// Il emet les deux formes d'annonce a la fois, ce qu'aucun artefact amont ne faisait : la
    /// sonde rendait alors `["VULKAN", "Vulkan", "CPU"]`, soit le meme dos deux fois sur l'ecran
    /// de mesure, orthographie differemment. Le cas n'etait couvert par aucun test parce qu'il
    /// n'existait pas avant qu'on construise l'artefact nous-memes.
    #[test]
    fn l_artefact_vulkan_maison_n_annonce_pas_deux_fois_le_meme_dos() {
        let reel = "ggml_vulkan: Found 1 Vulkan devices:\n\
                    ggml_vulkan: 0 = NVIDIA GeForce GTX 1060 6GB (NVIDIA) | uma: 0 | fp16: 0\n\
                    load_backend: loaded Vulkan backend from C:\\x\\ggml-vulkan.dll\n\
                    load_backend: loaded CPU backend from C:\\x\\ggml-cpu-haswell.dll\n";
        assert_eq!(annonces(reel), vec!["VULKAN", "CPU"]);
    }

    /// ⛔ **Garde la panne qu'aucune machine de developpement ne peut montrer.** Sur un Windows
    /// sans redistribuable Visual C++, le moteur ne demarre pas : l'installateur depose le runtime
    /// a cote de `oyant.exe`, mais Windows cherche dans le repertoire du binaire LANCE. Ce
    /// `PATH` est le seul lien entre les deux.
    #[test]
    fn le_repertoire_de_l_application_passe_en_tete_du_chemin() {
        let repertoire = Path::new("C:\\Program Files\\Oyant");

        let enrichi = chemin_enrichi("C:\\Windows\\System32;C:\\autre", repertoire);
        assert!(enrichi.starts_with("C:\\Program Files\\Oyant;"));
        assert!(enrichi.ends_with("C:\\Windows\\System32;C:\\autre"));

        // Un PATH vide ne doit pas produire de separateur orphelin.
        assert_eq!(chemin_enrichi("", repertoire), "C:\\Program Files\\Oyant");
    }

    /// ⛔ **Prouve que le `PATH` calcule atteint VRAIMENT le processus enfant**, et pas seulement
    /// que la fonction pure rend la bonne chaine. C'est la moitie du mecanisme qu'un test de
    /// chaine ne peut pas couvrir : un `.env()` pose sur la mauvaise commande, ou ecrase plus
    /// loin, laisserait la fonction pure verte et le moteur sans son runtime.
    #[cfg(target_os = "windows")]
    #[test]
    fn le_processus_enfant_recoit_le_chemin_enrichi() {
        // Un repertoire sentinelle, qui ne peut pas s'etre retrouve la par accident.
        let sortie = commande_avec(
            Path::new("cmd"),
            "C:\\Windows\\System32",
            Path::new("Z:\\Oyant-sentinelle"),
        )
        .args(["/c", "echo %PATH%"])
        .output()
        .expect("cmd doit pouvoir etre lance");

        let vu = String::from_utf8_lossy(&sortie.stdout);
        assert_eq!(
            vu.trim(),
            "Z:\\Oyant-sentinelle;C:\\Windows\\System32",
            "le PATH n'est pas arrive intact au processus enfant"
        );
    }

    /// ⚠️ Trois appels de moteur par transcription : sans cette garde, le `PATH` s'allongerait
    /// d'une copie a chaque fois, jusqu'a depasser la limite de l'environnement d'un processus.
    #[test]
    fn le_chemin_ne_recoit_pas_deux_fois_le_meme_repertoire() {
        let repertoire = Path::new("C:\\Program Files\\Oyant");
        let une_fois = chemin_enrichi("C:\\Windows\\System32", repertoire);
        assert_eq!(chemin_enrichi(&une_fois, repertoire), une_fois);

        // Meme repertoire, ecrit autrement : Windows ne fait pas la difference, nous non plus.
        let casse = chemin_enrichi("c:\\program files\\oyant;C:\\Windows", repertoire);
        assert_eq!(casse, "c:\\program files\\oyant;C:\\Windows");
    }

    /// ⛔ **CUDA n'est propose que si le pilote NVIDIA est la** (arbitre par painteau le
    /// 2026-09-17). Sans ce filtre, un ecran propose 675 Mo de telechargement a quelqu'un dont la
    /// machine ne peut rien en faire, et le seul symptome serait une transcription aussi lente
    /// qu'avant.
    #[cfg(target_os = "windows")]
    #[test]
    fn cuda_disparait_quand_le_pilote_nvidia_est_absent() {
        let nvidia = par_identifiant("windows-x64-nvidia").unwrap();
        let vulkan = par_identifiant("windows-x64-amd-intel").unwrap();
        let processeur = par_identifiant("windows-x64-cpu").unwrap();

        let sans = vec![];
        assert!(!pertinent(nvidia, &sans, ""), "CUDA propose sans pilote");
        // Les deux autres ne dependent d'aucun pilote et restent proposes partout.
        assert!(pertinent(vulkan, &sans, ""));
        assert!(pertinent(processeur, &sans, ""));

        let avec = vec!["nvcuda.dll".to_string()];
        assert!(pertinent(nvidia, &avec, ""));
        // La casse d'un nom de fichier Windows ne doit pas decider de ce qu'on affiche.
        let majuscules = vec!["NVCUDA.DLL".to_string()];
        assert!(pertinent(nvidia, &majuscules, ""));
    }

    /// ⛔ **Sauf s'il est le choix courant.** Les reglages suivent l'utilisateur d'une machine a
    /// l'autre : masquer le moteur qui calcule vraiment donnerait un ecran ou aucune option n'est
    /// marquee « utilisé », sans rien dire de ce qui tourne.
    #[cfg(target_os = "windows")]
    #[test]
    fn le_choix_courant_reste_visible_meme_devenu_hors_sujet() {
        let nvidia = par_identifiant("windows-x64-nvidia").unwrap();
        assert!(pertinent(nvidia, &[], "windows-x64-nvidia"));
        assert!(!pertinent(nvidia, &[], "windows-x64-cpu"));
    }

    /// ⚠️ Meme exigence que pour les modeles : une empreinte mal recopiee ferait refuser une
    /// archive saine en accusant le reseau.
    #[test]
    fn chaque_empreinte_est_un_sha256_bien_forme() {
        for moteur in MOTEURS.iter().filter(|m| m.disponible && !m.embarque) {
            assert_eq!(moteur.empreinte.len(), 64, "{}", moteur.identifiant);
            assert!(moteur.empreinte.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(moteur.empreinte.chars().all(|c| !c.is_ascii_uppercase()));
            assert!(moteur.url.starts_with("https://"));
            assert!(moteur.taille > 1_000_000);
        }
    }

    /// ⚠️ La regle inverse : un choix qu'on ne peut pas installer doit DIRE pourquoi, et un choix
    /// installable ne doit pas trainer une raison d'indisponibilite perimee. Les deux etats sont
    /// coherents ou l'ecran ment.
    #[test]
    fn un_choix_indisponible_porte_sa_raison_et_rien_d_autre() {
        for moteur in MOTEURS {
            if moteur.disponible {
                assert!(
                    moteur.indisponible_parce_que.is_empty(),
                    "{}",
                    moteur.identifiant
                );
                // Un moteur embarque n'a pas d'URL : il ne se telecharge pas.
                assert_eq!(
                    moteur.url.is_empty(),
                    moteur.embarque,
                    "{} : URL et embarque se contredisent",
                    moteur.identifiant
                );
            } else {
                assert!(
                    !moteur.indisponible_parce_que.is_empty(),
                    "{} : indisponible sans raison",
                    moteur.identifiant
                );
                assert!(moteur.url.is_empty(), "{}", moteur.identifiant);
            }
        }
    }

    /// ⛔ **Le plancher du produit n'est pas le meme partout, et l'ecrire ici evite de le
    /// decouvrir en production.**
    ///
    /// Ce test affirmait « exactement un moteur embarque » pour **toutes** les plateformes. C'est
    /// une supposition Windows : elle est restee vraie tant que Windows etait la seule cible avec
    /// un catalogue, et elle est devenue fausse le jour ou Linux en a eu un. Le defaut est reste
    /// invisible trois jours parce que l'integration continue etait deja rouge pour une autre
    /// raison.
    ///
    /// ⚠️ La difference de fond, qui se dit a l'utilisateur et pas seulement au compilateur :
    /// **sous Windows, Oyant dicte des la fin de l'installation, sans reseau** ; sous Linux, la
    /// premiere dictee demande un telechargement.
    #[test]
    fn le_plancher_de_chaque_plateforme_est_celui_qu_on_annonce() {
        let embarques = MOTEURS.iter().filter(|m| m.embarque).count();

        if cfg!(target_os = "windows") {
            // Zero rendrait le premier lancement dependant du telechargement ; deux se
            // disputeraient le meme repertoire.
            assert_eq!(embarques, 1, "Windows doit embarquer exactement un moteur");
            assert!(MOTEURS.iter().any(|m| m.disponible));
        } else if cfg!(target_os = "linux") {
            // L'amont publie une archive Linux, mais elle ne voyage pas dans notre installateur.
            assert_eq!(embarques, 0, "aucun moteur n'est embarque sous Linux");
            assert!(
                MOTEURS.iter().any(|m| m.disponible),
                "Linux doit avoir un moteur telechargeable"
            );
        } else {
            // macOS : l'amont ne publie qu'un `xcframework`, donc aucun programme a appeler. Le
            // catalogue est vide tant que nous n'avons pas construit le notre, et un catalogue
            // vide est honnete ; inventer une entree qui echouerait ne le serait pas.
            assert!(MOTEURS.is_empty(), "macOS n'a pas encore de moteur");
        }
    }

    /// ⛔ **Ce test garde le piege le plus couteux du module.** Les etiquettes de VERSION de
    /// whisper.cpp (`v1.9.4`) ne portent aucun binaire ; seules les etiquettes de compilation
    /// (`b5130`) en ont. Une URL pointant sur une etiquette de version donnerait un 404 dont le
    /// message accuserait le reseau.
    #[cfg(target_os = "windows")]
    #[test]
    fn les_urls_pointent_sur_une_etiquette_de_compilation() {
        for moteur in MOTEURS
            .iter()
            .filter(|m| m.disponible && !m.embarque && m.url.contains("/releases/download/"))
        {
            let apres = moteur.url.split("/releases/download/").nth(1).unwrap();
            let etiquette = apres.split('/').next().unwrap();
            assert!(
                etiquette.starts_with('b') && etiquette[1..].chars().all(|c| c.is_ascii_digit()),
                "{} : « {etiquette} » n'est pas une etiquette de compilation",
                moteur.identifiant
            );
        }
    }

    /// ⛔ **La contrepartie, pour les artefacts qu'on heberge nous-memes.** Le catalogue epingle
    /// une empreinte, donc republier sous le meme nom rendrait non installables toutes les
    /// versions deja distribuees, et le defaut ne se verrait qu'a la mise a jour SUIVANTE. La
    /// convention de parc (`admin/docs/distribution.md`) nomme ca la copie versionnee : on
    /// l'impose ici plutot que de compter sur le souvenir qu'on en a le jour de la republication.
    #[cfg(target_os = "windows")]
    #[test]
    fn nos_propres_artefacts_portent_leur_version_dans_leur_nom() {
        for moteur in MOTEURS
            .iter()
            .filter(|m| m.disponible && !m.embarque && m.url.contains("dl.breizhzion.com"))
        {
            let nom = moteur.url.rsplit('/').next().unwrap();
            let porte_une_version = nom
                .trim_end_matches(".zip")
                .rsplit('-')
                .next()
                .map(|fin| fin.starts_with('b') && fin[1..].chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false);
            assert!(
                porte_une_version,
                "{} : « {nom} » est un nom fixe, il serait ecrase a la prochaine version",
                moteur.identifiant
            );
        }
    }

    // ── Le prompt de vocabulaire ───────────────────────────────────────────────────────────

    fn mots(nombre: usize, longueur: usize) -> Vec<String> {
        (0..nombre)
            .map(|i| format!("{}{i:03}", "x".repeat(longueur - 3)))
            .collect()
    }

    /// ⚠️ `None` et non une chaine vide : on ne veut pas passer `--prompt ""` au moteur, qui le
    /// compterait quand meme comme du contexte.
    #[test]
    fn un_vocabulaire_vide_ne_donne_aucun_prompt() {
        assert_eq!(prompt_de_vocabulaire(&[]), None);
        assert_eq!(prompt_de_vocabulaire(&["".to_string()]), None);
        assert_eq!(
            prompt_de_vocabulaire(&["   ".to_string(), "\t".to_string()]),
            None
        );
    }

    #[test]
    fn les_entrees_sont_jointes_et_debarrassees_de_leurs_espaces() {
        let entrees = vec![
            "  Kowalczyk ".to_string(),
            "Lévothyrox".to_string(),
            "".to_string(),
            " Villeurbanne".to_string(),
        ];
        assert_eq!(
            prompt_de_vocabulaire(&entrees).as_deref(),
            Some("Kowalczyk, Lévothyrox, Villeurbanne")
        );
    }

    /// ⛔ Le point qui compte : la troncature tombe entre deux entrees, jamais au milieu d'un mot.
    /// Un prompt coupe sur « Kowal » biaiserait le moteur vers un fragment inexistant, ce qui est
    /// pire que de ne pas donner l'entree.
    #[test]
    fn le_plafond_tronque_a_la_frontiere_d_une_entree() {
        // Des mots de 20 caracteres : 30 entrees depassent largement les 600 caracteres.
        let entrees = mots(30, 20);
        let prompt = prompt_de_vocabulaire(&entrees).expect("un prompt est attendu");

        assert!(
            prompt.chars().count() <= PLAFOND_PROMPT,
            "prompt de {} caracteres, au-dela du plafond de {PLAFOND_PROMPT}",
            prompt.chars().count()
        );

        // Chaque morceau garde doit etre une entree ENTIERE de la liste d'origine.
        for morceau in prompt.split(", ") {
            assert!(
                entrees.iter().any(|e| e == morceau),
                "« {morceau} » n'est pas une entree complete : la coupe est tombee dans un mot"
            );
        }

        // Et on garde bien le debut de la liste, pas une poignee au hasard.
        assert!(
            prompt.starts_with(&entrees[0]),
            "la premiere entree doit etre gardee"
        );
    }

    /// Une seule entree plus longue que le plafond ne doit pas etre coupee en deux : on n'en
    /// garde aucune, et on le dit par `None` plutot que par un fragment.
    #[test]
    fn une_entree_plus_longue_que_le_plafond_est_ecartee_entierement() {
        let enorme = "y".repeat(PLAFOND_PROMPT + 1);
        assert_eq!(prompt_de_vocabulaire(&[enorme]), None);
    }

    /// Le plafond ne doit pas ecarter une liste raisonnable : le cas reel est une poignee de noms
    /// propres, et il doit passer en entier.
    #[test]
    fn un_vocabulaire_de_taille_realiste_passe_en_entier() {
        let entrees: Vec<String> = [
            "Kowalczyk",
            "Lévothyrox",
            "Villeurbanne",
            "Breizhzion",
            "Oliveira",
            "amoxicilline",
            "Bodhrán",
            "Szczepański",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let prompt = prompt_de_vocabulaire(&entrees).expect("un prompt est attendu");
        for entree in &entrees {
            assert!(
                prompt.contains(entree.as_str()),
                "« {entree} » a ete ecartee a tort"
            );
        }
    }

    // ── Le vocabulaire deduit des substitutions ────────────────────────────────────────────

    fn substitution(cherche: &str, remplace: &str) -> crate::texte::Substitution {
        crate::texte::Substitution {
            cherche: cherche.to_string(),
            remplace: remplace.to_string(),
            sensible_casse: false,
        }
    }

    /// Le cas qui justifie toute la fonction : l'utilisateur a du corriger un nom a la main, donc
    /// le moteur s'est trompe dessus, donc c'est exactement ce qu'il faut lui donner avant.
    #[test]
    fn une_cible_de_substitution_qui_est_un_nom_devient_du_vocabulaire() {
        let termes = termes_des_substitutions(&[
            substitution("lévotirox", "Lévothyrox"),
            substitution("e c g", "ECG"),
            substitution("mac kenzie", "McKenzie"),
        ]);
        assert_eq!(termes, vec!["Lévothyrox", "ECG", "McKenzie"]);
    }

    /// ⛔ Le contre-cas, et c'est lui qui protege la transcription : une substitution de tournure
    /// ou de ponctuation n'est pas du vocabulaire. La retenir mangerait le plafond du prompt et
    /// biaiserait le moteur vers une expression que personne n'a prononcee.
    #[test]
    fn une_correction_de_tournure_ne_devient_pas_du_vocabulaire() {
        let termes = termes_des_substitutions(&[
            // Aucune majuscule : une tournure, pas un terme.
            substitution("nest ce pas", "n'est-ce pas ?"),
            substitution("cad", "c'est-à-dire"),
            // Une majuscule, mais bien trop de mots pour etre un terme.
            substitution(
                "formule",
                "Je vous prie d'agréer mes salutations distinguées",
            ),
            // Une majuscule, peu de mots, mais trop long.
            substitution("long", &format!("A{}", "z".repeat(LONGUEUR_MAX_TERME))),
            // Vide apres nettoyage.
            substitution("rien", "   "),
        ]);
        assert!(
            termes.is_empty(),
            "des tournures ont ete prises pour du vocabulaire : {termes:?}"
        );
    }

    /// ⚠️ L'ordre decide de ce qui survit a la troncature : ce que l'utilisateur a saisi passe
    /// devant ce qu'on a deduit pour lui.
    #[test]
    fn le_vocabulaire_saisi_passe_avant_les_termes_deduits() {
        let reglages = crate::reglages::Reglages {
            vocabulaire: vec!["Szczepański".to_string()],
            substitutions: vec![substitution("lévotirox", "Lévothyrox")],
            ..Default::default()
        };

        let prompt = prompt_des_reglages(&reglages).expect("un prompt est attendu");
        assert_eq!(prompt, "Szczepański, Lévothyrox");
    }

    /// Donner deux fois le meme terme au moteur gaspillerait le plafond pour rien.
    #[test]
    fn un_terme_present_des_deux_cotes_n_est_donne_qu_une_fois() {
        let reglages = crate::reglages::Reglages {
            vocabulaire: vec!["ECG".to_string()],
            // ⚠️ « Ecg » porte une majuscule, donc il PASSE le filtre et atteint bien la
            // deduplication. Une cible en minuscules aurait ete ecartee avant, et le test aurait
            // ete vert sans jamais exercer ce qu'il pretend garder.
            substitutions: vec![substitution("e c g", "Ecg")],
            ..Default::default()
        };

        let prompt = prompt_des_reglages(&reglages).expect("un prompt est attendu");
        assert_eq!(prompt, "ECG");
    }

    /// Des reglages neufs ne donnent aucun prompt : l'historique est a zero, le vocabulaire aussi,
    /// et le moteur ne doit recevoir aucune option supplementaire.
    #[test]
    fn des_reglages_par_defaut_ne_donnent_aucun_prompt() {
        assert_eq!(
            prompt_des_reglages(&crate::reglages::Reglages::default()),
            None
        );
    }

    fn demande_essai<'a>(famille: crate::modeles::Famille, prompt: Option<&'a str>) -> Demande<'a> {
        Demande {
            famille,
            modele: Path::new("modele.bin"),
            audio: Path::new("audio.wav"),
            langue: "fr",
            fils: 4,
            temperature: 0.0,
            prompt,
        }
    }

    fn en_texte(arguments: Vec<std::ffi::OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// ⛔ `parakeet-cli` refuse de demarrer sur une option qu'il ne connait pas : aucune option
    /// propre a Whisper ne doit lui parvenir, meme quand les reglages en portent.
    #[test]
    fn parakeet_ne_recoit_aucune_option_de_whisper() {
        let arguments = en_texte(arguments(&demande_essai(
            crate::modeles::Famille::Parakeet,
            Some("ECG, Kowalski"),
        )));
        for interdite in [
            "-l",
            "--temperature",
            "--prompt",
            "--carry-initial-prompt",
            "--no-timestamps",
        ] {
            assert!(
                !arguments.iter().any(|a| a == interdite),
                "{interdite} ne doit pas partir vers Parakeet : {arguments:?}"
            );
        }
        assert!(arguments.contains(&"--no-prints".to_string()));
    }

    /// La reecriture en `Demande` ne doit rien avoir retire a Whisper.
    #[test]
    fn whisper_garde_langue_temperature_et_vocabulaire() {
        let arguments = en_texte(arguments(&demande_essai(
            crate::modeles::Famille::Whisper,
            Some("ECG"),
        )));
        let suivant = |option: &str| {
            let position = arguments.iter().position(|a| a == option)?;
            arguments.get(position + 1).cloned()
        };
        assert_eq!(suivant("-l").as_deref(), Some("fr"));
        assert_eq!(suivant("--temperature").as_deref(), Some("0"));
        assert_eq!(suivant("--prompt").as_deref(), Some("ECG"));
        assert!(arguments.contains(&"--carry-initial-prompt".to_string()));
        assert!(arguments.contains(&"--no-timestamps".to_string()));
    }

    /// ⛔ **Le moteur embarque n'a Parakeet que si le script qui le prepare le garde.** Les deux
    /// vivent dans des fichiers differents : sans ce test, retirer `parakeet-cli.exe` du jeu
    /// minimal laisserait le catalogue promettre un programme absent de l'installateur, et le
    /// defaut ne se verrait qu'a la premiere dictee d'un utilisateur.
    #[cfg(target_os = "windows")]
    #[test]
    fn le_script_du_moteur_embarque_garde_parakeet() {
        let embarque = MOTEURS
            .iter()
            .find(|m| m.embarque)
            .expect("un moteur embarque");
        let script = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("scripts")
                .join("preparer-moteur-embarque.py"),
        )
        .expect("script lisible");
        let bibliotheque = embarque.executable_parakeet.replace("-cli.exe", ".dll");
        for fichier in [embarque.executable_parakeet, bibliotheque.as_str()] {
            assert!(
                script.contains(&format!("\"{fichier}\"")),
                "{fichier} manque au jeu minimal de preparer-moteur-embarque.py"
            );
        }
    }

    /// Un moteur sans Parakeet le dit, au lieu de rendre le binaire de Whisper.
    #[test]
    fn un_moteur_sans_parakeet_rend_une_erreur() {
        for moteur in MOTEURS.iter().filter(|m| m.executable_parakeet.is_empty()) {
            assert!(executable_pour(moteur, crate::modeles::Famille::Parakeet).is_err());
        }
    }
}
