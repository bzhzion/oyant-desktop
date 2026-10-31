//! Telechargement et verification des modeles de transcription.
//!
//! ⛔ **Le fichier ne se contente pas d'exister : il doit etre le BON.** Un modele tronque par une
//! coupure reseau garde un nom parfaitement normal, et le moteur echouerait plus tard avec un
//! message qui ne parlerait pas de telechargement. C'est le defaut que ce module existe pour
//! rendre impossible.
//!
//! Trois etats, et la distinction compte :
//!
//! - **absent** : rien sur le disque,
//! - **present** : le fichier fait la taille attendue, ce qui ne prouve rien d'autre,
//! - **verifie** : son empreinte SHA-256 a ete calculee et correspond.
//!
//! ⚠️ **L'empreinte n'est PAS recalculee a chaque ouverture.** Lire 3 Go a chaque lancement pour
//! reafficher un ecran serait inacceptable, et c'est exactement le defaut rencontre sur l'app iOS,
//! ou une verification par empreinte complete a du etre remplacee par une comparaison de taille.
//! Ici on garde les deux : une marque `.verifie` note le resultat, et un bouton permet de rejouer
//! la verification quand on la veut.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

/// Un modele qu’Oyant sait telecharger.
///
/// ⚠️ **Tailles et empreintes RELEVEES sur la source**, jamais estimees : lues le 2026-09-17 sur
/// l'API de HuggingFace, qui expose l'identifiant LFS de chaque fichier, lequel est son SHA-256.
/// Une empreinte inventee rendrait ce module pire qu'inutile, puisqu'il refuserait des fichiers
/// parfaitement sains.
pub struct Modele {
    pub identifiant: &'static str,
    pub nom: &'static str,
    pub fichier: &'static str,
    pub url: &'static str,
    pub taille: u64,
    pub empreinte: &'static str,
    /// Ce qu'on en dit a l'utilisateur pour qu'il choisisse sans connaitre le domaine.
    pub description: &'static str,
    /// Le programme qui sait lire ce modele.
    pub famille: Famille,
    /// Les langues couvertes, telles qu'on les dit a l'ecran.
    pub langues: &'static str,
}

/// Ce que `scripts/mesurer-modeles.py` a mesure, integre a la compilation.
///
/// ⛔ **Ces chiffres ne s'ecrivent jamais a la main** : un nombre tape a cote du code qu'il decrit
/// finit par mentir. Le fichier dit lui-meme sur quelle machine et quel corpus il a ete produit,
/// et l'ecran le repete sous le tableau.
const MESURES: &str = include_str!("../mesures-modeles.json");

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Mesure {
    pub fois_plus_rapide_que_la_parole: f64,
    /// `None` pour un modele qui ne transcrit pas le francais : le corpus de mesure est en
    /// francais, et un taux d'erreurs sur une langue que le modele ne connait pas serait un chiffre
    /// juste et trompeur. L'ecran le dit en toutes lettres.
    pub erreurs_pour_cent: Option<f64>,
    pub erreurs_pour_cent_pire_voix: Option<f64>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct ContexteMesures {
    pub date: String,
    pub machine: String,
    pub corpus: String,
    pub script: String,
}

#[derive(serde::Deserialize)]
struct FichierMesures {
    mesure: ContexteMesures,
    modeles: std::collections::BTreeMap<String, Mesure>,
}

fn fichier_mesures() -> Option<FichierMesures> {
    serde_json::from_str(MESURES).ok()
}

/// La mesure d'un modele, `None` s'il n'a pas ete mesure.
pub fn mesure(identifiant: &str) -> Option<Mesure> {
    fichier_mesures()?.modeles.remove(identifiant)
}

/// Dans quelles conditions les mesures ont ete prises, pour la legende du tableau.
#[tauri::command]
pub fn contexte_mesures() -> Option<ContexteMesures> {
    fichier_mesures().map(|f| f.mesure)
}

/// Deux familles de modeles, deux programmes, et surtout deux jeux d'options.
///
/// ⛔ **Parakeet n'est pas un Whisper plus rapide, c'est un autre moteur.** `parakeet-cli` n'a ni
/// `-l`, ni `--prompt`, ni `--temperature` : lui passer les options de Whisper le ferait echouer,
/// et les taire sans le dire ferait croire que la langue choisie et le vocabulaire s'appliquent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Famille {
    Whisper,
    Parakeet,
    /// Lu par `sherpa-onnx-offline` (`sherpa.rs`), pas par whisper.cpp.
    SenseVoice,
}

/// Les langues que Parakeet v3 sait transcrire, en codes ISO 639-1.
///
/// ⚠️ **Relevees sur la fiche du modele chez NVIDIA, pas deduites** : 25 langues europeennes, et
/// rien d'autre. Le coreen n'y est pas, alors que c'est une langue que le produit propose et
/// qu'on parle dans le foyer de son auteur. D'ou un refus explicite avant de lancer quoi que ce
/// soit, plutot qu'un texte phonetique dans une autre langue, tape dans l'application de
/// l'utilisateur.
pub const LANGUES_PARAKEET: &[&str] = &[
    "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "hr", "hu", "it", "lt", "lv", "mt",
    "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "uk",
];

/// Les langues de SenseVoice, relevees sur la fiche du modele : coreen, chinois, japonais,
/// anglais et cantonais. ⚠️ **Ni le francais ni aucune autre langue europeenne hors l'anglais** :
/// c'est le modele rapide pour l'Asie de l'Est, pas un remplacant de Whisper.
pub const LANGUES_SENSEVOICE: &[&str] = &["ko", "zh", "ja", "en", "yue"];

impl Famille {
    /// Cette famille sait-elle transcrire la langue demandee ?
    ///
    /// `auto` est toujours accepte : Parakeet detecte seul parmi ses langues, sans option pour
    /// le lui dire.
    pub fn connait(self, langue: &str) -> bool {
        match self {
            Famille::Whisper => true,
            Famille::Parakeet => langue == "auto" || LANGUES_PARAKEET.contains(&langue),
            Famille::SenseVoice => langue == "auto" || LANGUES_SENSEVOICE.contains(&langue),
        }
    }
}

pub const CATALOGUE: &[Modele] = &[
    Modele {
        identifiant: "small",
        nom: "Small",
        fichier: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        taille: 487_601_967,
        empreinte: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        description: "Le plus rapide, et le plus leger a telecharger. Suffisant pour dicter des phrases courtes.",
        famille: Famille::Whisper,
        langues: "99 langues",
    },
    Modele {
        identifiant: "medium",
        nom: "Medium",
        fichier: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        taille: 1_533_763_059,
        empreinte: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        description: "Le bon compromis entre precision et rapidite pour un usage quotidien.",
        famille: Famille::Whisper,
        langues: "99 langues",
    },
    Modele {
        identifiant: "large-v3",
        nom: "Large v3",
        fichier: "ggml-large-v3.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        taille: 3_095_033_483,
        empreinte: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        description: "Le plus precis, mais lourd a telecharger et plus lent a la transcription.",
        famille: Famille::Whisper,
        langues: "99 langues",
    },
    // ⚠️ **q8_0 et pas f16** : mesure le 2026-10-29 sur trois lecteurs LibriVox en francais,
    // les deux rendent le MEME texte au caractere pres, et q8_0 pese moitie moins (669 Mo contre
    // 1,26 Go). Rien a gagner a faire telecharger le double.
    Modele {
        identifiant: "parakeet-v3",
        nom: "Parakeet v3",
        fichier: "ggml-parakeet-tdt-0.6b-v3-q8_0.bin",
        url: "https://huggingface.co/ggml-org/parakeet-GGUF/resolve/main/ggml-parakeet-tdt-0.6b-v3-q8_0.bin",
        taille: 668_757_119,
        empreinte: "4d64e9e96c2792186d072fde0034df0ad670cf680a2f53069052ead827fd600e",
        // ⛔ Les deux limites sont DANS la description, parce que c'est la que se fait le choix :
        // les decouvrir apres coup, c'est croire que le vocabulaire ou le coreen sont casses.
        description: "Deux à trois fois plus rapide que Small, souvent plus juste en français. Langues européennes seulement (pas de coréen), sans vocabulaire personnalisé. Utilise la carte graphique si vous l'avez choisie.",
        famille: Famille::Parakeet,
        langues: "25 langues européennes, sans le coréen",
    },
    // SenseVoice Small, export ONNX quantifie en int8 par sherpa-onnx. Taille et empreinte LUES
    // sur l'API HuggingFace (identifiant LFS) le 2026-10-31.
    Modele {
        identifiant: "sensevoice",
        nom: "SenseVoice",
        fichier: "sensevoice-small-2024-07-17-int8.onnx",
        url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main/model.int8.onnx",
        taille: 239_233_841,
        empreinte: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
        description: "Très rapide, pour le coréen, le chinois, le japonais et l'anglais. Ne transcrit pas le français. Installe aussi son propre programme (21 Mo) au premier choix.",
        famille: Famille::SenseVoice,
        langues: "Coréen, chinois, japonais, anglais et cantonais",
    },
];

pub fn par_identifiant(identifiant: &str) -> Option<&'static Modele> {
    CATALOGUE.iter().find(|m| m.identifiant == identifiant)
}

/// Ce que l'interface affiche pour un modele.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EtatModele {
    pub identifiant: String,
    pub nom: String,
    pub description: String,
    pub taille_attendue: u64,
    /// Octets reellement sur le disque, fichier complet ou telechargement interrompu.
    pub octets_locaux: u64,
    /// Le fichier fait la taille attendue. **Ne prouve pas** qu'il est intact.
    pub complet: bool,
    /// Son empreinte a ete calculee et correspond.
    pub verifie: bool,
    /// Un telechargement a ete interrompu et pourra reprendre ou il s'est arrete.
    pub reprise_possible: bool,
    /// C'est CE modele qui sert aux transcriptions.
    ///
    /// ⛔ Le champ existe parce que la liste ne le disait pas. Le modele ne se choisissait que
    /// dans une liste deroulante des reglages, donc a un endroit different de celui ou on le
    /// telecharge : painteau a dicte sans savoir avec quoi, le 2026-09-18. Un reglage present a
    /// deux endroits, ou absent de celui ou on le cherche, revient au meme defaut.
    pub actif: bool,
    pub langues: String,
    /// Vitesse et taux d'erreurs mesures par `scripts/mesurer-modeles.py`, `None` si absent.
    pub mesure: Option<Mesure>,
}

/// Progression envoyee a l'interface pendant un telechargement.
#[derive(Clone, Serialize)]
struct Progression {
    identifiant: String,
    recus: u64,
    total: u64,
}

/// Demandes d'interruption en cours, une par modele.
///
/// ⚠️ Un telechargement de 3 Go qu'on ne peut pas arreter est un defaut a lui seul : sur une
/// connexion lente ou limitee, la seule issue serait de tuer l'application.
#[derive(Default)]
pub struct Interruptions(Mutex<Vec<(String, std::sync::Arc<AtomicBool>)>>);

impl Interruptions {
    fn drapeau(&self, identifiant: &str) -> std::sync::Arc<AtomicBool> {
        let mut liste = self.0.lock().unwrap();
        if let Some((_, drapeau)) = liste.iter().find(|(id, _)| id == identifiant) {
            return drapeau.clone();
        }
        let drapeau = std::sync::Arc::new(AtomicBool::new(false));
        liste.push((identifiant.to_string(), drapeau.clone()));
        drapeau
    }
}

/// Repertoire des modeles.
///
/// ⛔ **`app_local_data_dir` et surtout pas `app_data_dir`.** Sur Windows, le second est dans le
/// profil ITINERANT : dans un domaine, ces 3 Go seraient recopies sur le reseau a chaque ouverture
/// de session. Le defaut ne se verrait pas du tout sur une machine personnelle, et rendrait
/// l'application inutilisable en entreprise.
///
/// ⛔ Et surtout pas dans le repertoire d'installation, que la desinstallation emporte : c'est ce
/// que faisait l'ancienne version, et c'est ce qui a produit la collision du 2026-09-17.
fn dossier(_app: &AppHandle) -> Result<PathBuf, String> {
    // ⚠️ Passe par `chemins`, PAS par `app.path()` : la ligne de commande n'a pas d'application
    // Tauri sous la main, et deux calculs separes finiraient par designer deux repertoires
    // differents. Voir `chemins.rs`.
    crate::chemins::modeles()
}

fn chemin(app: &AppHandle, modele: &Modele) -> Result<PathBuf, String> {
    Ok(dossier(app)?.join(modele.fichier))
}

fn chemin_partiel(app: &AppHandle, modele: &Modele) -> Result<PathBuf, String> {
    Ok(dossier(app)?.join(format!("{}.partiel", modele.fichier)))
}

fn chemin_marque(app: &AppHandle, modele: &Modele) -> Result<PathBuf, String> {
    Ok(dossier(app)?.join(format!("{}.verifie", modele.fichier)))
}

fn taille_sur_disque(chemin: &PathBuf) -> u64 {
    fs::metadata(chemin).map(|m| m.len()).unwrap_or(0)
}

/// Calcule l'empreinte SHA-256 d'un fichier, par morceaux.
///
/// ⚠️ Lecture par blocs de 1 Mio et jamais d'un coup : charger 3 Go en memoire pour les hacher
/// ferait echouer la verification precisement sur les modeles ou elle sert le plus.
fn empreinte(chemin: &PathBuf) -> Result<String, String> {
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

    // ⚠️ Formatage hexadecimal a la main : depuis `sha2` 0.11, `finalize()` rend un `Array` qui
    // n'implemente plus `LowerHex`, donc `format!("{:x}", ...)` ne compile pas.
    let resume = hacheur.finalize();
    let mut hexadecimal = String::with_capacity(resume.len() * 2);
    for octet in resume.iter() {
        // Minuscules obligatoires : les empreintes du catalogue le sont, et la comparaison est
        // une egalite de chaines.
        hexadecimal.push_str(&format!("{octet:02x}"));
    }
    Ok(hexadecimal)
}

fn etat_de(app: &AppHandle, modele: &Modele, choisi: &str) -> EtatModele {
    let fichier = chemin(app, modele).unwrap_or_default();
    let partiel = chemin_partiel(app, modele).unwrap_or_default();
    let marque = chemin_marque(app, modele).unwrap_or_default();

    let octets_fichier = taille_sur_disque(&fichier);
    let octets_partiels = taille_sur_disque(&partiel);
    let complet = octets_fichier == modele.taille && modele.taille > 0;

    // ⚠️ La marque n'est crue que si elle porte l'empreinte ATTENDUE **et** que le fichier fait
    // toujours la bonne taille. Une marque qui survivrait a un remplacement du fichier
    // certifierait un contenu qui n'est plus la.
    let verifie = complet
        && fs::read_to_string(&marque)
            .map(|contenu| contenu.trim() == modele.empreinte)
            .unwrap_or(false);

    EtatModele {
        identifiant: modele.identifiant.to_string(),
        nom: modele.nom.to_string(),
        description: modele.description.to_string(),
        taille_attendue: modele.taille,
        actif: modele.identifiant == choisi,
        octets_locaux: if complet {
            octets_fichier
        } else {
            octets_partiels
        },
        complet,
        verifie,
        reprise_possible: !complet && octets_partiels > 0,
        langues: modele.langues.to_string(),
        mesure: mesure(modele.identifiant),
    }
}

/// Etat de tous les modeles du catalogue.
#[tauri::command]
pub fn etat_modeles(app: AppHandle) -> Vec<EtatModele> {
    // Lu UNE fois pour toute la liste : le relire par modele multiplierait les acces disque
    // pour une valeur qui ne peut pas changer entre deux lignes du meme affichage.
    let choisi = crate::reglages::lire_sans_application().modele;
    CATALOGUE
        .iter()
        .map(|m| etat_de(&app, m, &choisi))
        .collect()
}

/// Retient le modele choisi, apres s'etre assure qu'il est bien la.
///
/// ⛔ **L'ordre compte, et c'est le meme que pour le moteur : on telecharge AVANT d'enregistrer
/// le choix.** Enregistrer d'abord laisserait, si le telechargement echoue, un reglage qui
/// designe un fichier absent : Oyant refuserait alors de transcrire en disant que le modele
/// manque, sur un choix que l'utilisateur vient pourtant de faire.
#[tauri::command]
pub async fn choisir_modele(
    app: AppHandle,
    identifiant: String,
) -> Result<Vec<EtatModele>, String> {
    let modele = par_identifiant(&identifiant).ok_or("Modèle inconnu")?;

    let fichier = chemin(&app, modele)?;
    if taille_sur_disque(&fichier) != modele.taille {
        telecharger_modele(app.clone(), identifiant.clone()).await?;
    }

    // ⚠️ SenseVoice a besoin de son propre programme : l'installer ICI, au choix du modele, et pas
    // a la premiere dictee, ou l'on attendrait un telechargement apres avoir deja parle.
    if modele.famille == Famille::SenseVoice && crate::sherpa::executable().is_none() {
        tauri::async_runtime::spawn_blocking(crate::sherpa::installer)
            .await
            .map_err(|erreur| format!("Installation interrompue : {erreur}"))??;
    }

    let mut reglages = crate::reglages::lire_sans_application();
    reglages.modele = identifiant;
    crate::reglages::ecrire_reglages(app.clone(), reglages)?;

    Ok(etat_modeles(app))
}

/// Recalcule l'empreinte et met la marque a jour.
///
/// C'est ce qui permet de repondre a « ce fichier est-il encore bon ? » plutot qu'a « ce fichier
/// existe-t-il ? ». Volontairement declenche a la demande : c'est long.
#[tauri::command]
pub async fn verifier_modele(app: AppHandle, identifiant: String) -> Result<EtatModele, String> {
    let modele = par_identifiant(&identifiant).ok_or("Modèle inconnu")?;
    let fichier = chemin(&app, modele)?;

    if taille_sur_disque(&fichier) != modele.taille {
        return Err("Le fichier n'a pas la taille attendue : il est incomplet.".to_string());
    }

    let app_pour_calcul = app.clone();
    let modele_id = identifiant.clone();
    // Le calcul dure des secondes sur plusieurs Go : il ne doit pas bloquer l'interface.
    let calcule = tauri::async_runtime::spawn_blocking(move || {
        let m = par_identifiant(&modele_id).unwrap();
        let f = chemin(&app_pour_calcul, m)?;
        empreinte(&f)
    })
    .await
    .map_err(|erreur| format!("Vérification interrompue : {erreur}"))??;

    let marque = chemin_marque(&app, modele)?;
    if calcule == modele.empreinte {
        fs::write(&marque, modele.empreinte)
            .map_err(|erreur| format!("Marque non écrite : {erreur}"))?;
        Ok(etat_de(
            &app,
            modele,
            &crate::reglages::lire_sans_application().modele,
        ))
    } else {
        // ⚠️ La marque est retiree, pas laissee : sans ca l'ecran continuerait d'annoncer
        // « verifie » sur un fichier dont on vient de prouver qu'il ne l'est pas.
        let _ = fs::remove_file(&marque);
        Err(format!(
            "Le fichier est abîmé : son empreinte ne correspond pas à celle attendue. \
             Attendu {}…, obtenu {}….",
            &modele.empreinte[..12],
            &calcule[..12]
        ))
    }
}

/// Supprime un modele et sa marque.
#[tauri::command]
pub fn supprimer_modele(app: AppHandle, identifiant: String) -> Result<Vec<EtatModele>, String> {
    let modele = par_identifiant(&identifiant).ok_or("Modèle inconnu")?;
    for chemin in [
        chemin(&app, modele)?,
        chemin_partiel(&app, modele)?,
        chemin_marque(&app, modele)?,
    ] {
        if chemin.exists() {
            fs::remove_file(&chemin).map_err(|erreur| format!("Suppression refusée : {erreur}"))?;
        }
    }
    Ok(etat_modeles(app))
}

/// Demande l'arret du telechargement en cours.
#[tauri::command]
pub fn interrompre_modele(app: AppHandle, identifiant: String) {
    app.state::<Interruptions>()
        .drapeau(&identifiant)
        .store(true, Ordering::SeqCst);
}

/// Telecharge un modele, en reprenant ou il s'etait arrete.
///
/// ⚠️ **Le fichier ne prend son nom definitif qu'apres verification de son empreinte.** Tant qu'il
/// est en cours, il s'appelle `.partiel`. Consequence voulue : un fichier portant le nom du modele
/// a forcement ete verifie, donc rien ne peut se faire passer pour un modele valide.
#[tauri::command]
pub async fn telecharger_modele(app: AppHandle, identifiant: String) -> Result<EtatModele, String> {
    let modele = par_identifiant(&identifiant).ok_or("Modèle inconnu")?;
    let dossier = dossier(&app)?;
    fs::create_dir_all(&dossier).map_err(|erreur| format!("Répertoire non créé : {erreur}"))?;

    let drapeau = app.state::<Interruptions>().drapeau(&identifiant);
    drapeau.store(false, Ordering::SeqCst);

    let app_tache = app.clone();
    let id_tache = identifiant.clone();
    let drapeau_tache = drapeau.clone();

    let resultat: Result<(), String> = tauri::async_runtime::spawn_blocking(move || {
        let modele = par_identifiant(&id_tache).unwrap();
        let partiel = chemin_partiel(&app_tache, modele)?;
        let deja = taille_sur_disque(&partiel);

        let mut requete = ureq::get(modele.url);
        if deja > 0 {
            // Reprise : on ne redemande que ce qui manque.
            requete = requete.set("Range", &format!("bytes={deja}-"));
        }

        let reponse = requete
            .call()
            .map_err(|erreur| format!("Téléchargement impossible : {erreur}"))?;

        // ⚠️ 206 = le serveur a accepte la reprise. 200 = il renvoie tout depuis le debut, donc
        // il faut repartir de zero sous peine de coller la totalite du fichier a la suite de ce
        // qu'on avait deja, ce qui donnerait un fichier plus gros que prevu et illisible.
        let reprend = reponse.status() == 206;
        let mut deja = if reprend { deja } else { 0 };

        let mut sortie = if reprend && deja > 0 {
            fs::OpenOptions::new()
                .append(true)
                .open(&partiel)
                .map_err(|erreur| format!("Fichier non ouvert : {erreur}"))?
        } else {
            fs::File::create(&partiel).map_err(|erreur| format!("Fichier non créé : {erreur}"))?
        };

        let mut flux = reponse.into_reader();
        let mut tampon = vec![0u8; 256 * 1024];
        let mut dernier_signal = std::time::Instant::now();

        loop {
            if drapeau_tache.load(Ordering::SeqCst) {
                // On garde le `.partiel` : c'est lui qui rend la reprise possible.
                return Err("Téléchargement interrompu.".to_string());
            }

            let lus = flux
                .read(&mut tampon)
                .map_err(|erreur| format!("Transfert interrompu : {erreur}"))?;
            if lus == 0 {
                break;
            }

            std::io::Write::write_all(&mut sortie, &tampon[..lus])
                .map_err(|erreur| format!("Écriture impossible : {erreur}"))?;
            deja += lus as u64;

            // ⚠️ La progression est envoyee au plus dix fois par seconde. Un evenement par bloc
            // de 256 Kio, soit des milliers par seconde, saturerait l'interface et la rendrait
            // moins fluide que pas de progression du tout.
            if dernier_signal.elapsed().as_millis() >= 100 {
                let _ = app_tache.emit(
                    "modele-progression",
                    Progression {
                        identifiant: id_tache.clone(),
                        recus: deja,
                        total: modele.taille,
                    },
                );
                dernier_signal = std::time::Instant::now();
            }
        }

        drop(sortie);

        // ⚠️ On verifie AVANT de renommer. Un fichier tronque par une coupure a une taille
        // plausible et un nom parfaitement normal : si on renommait d'abord, il se ferait passer
        // pour un modele valide jusqu'a ce que le moteur echoue avec un message sans rapport.
        let calcule = empreinte(&partiel)?;
        if calcule != modele.empreinte {
            let _ = fs::remove_file(&partiel);
            return Err(
                "Le fichier téléchargé est abîmé : son empreinte ne correspond pas. \
                 Le téléchargement a été écarté, vous pouvez le relancer."
                    .to_string(),
            );
        }

        let final_ = chemin(&app_tache, modele)?;
        let _ = fs::remove_file(&final_);
        fs::rename(&partiel, &final_)
            .map_err(|erreur| format!("Mise en place impossible : {erreur}"))?;
        fs::write(chemin_marque(&app_tache, modele)?, modele.empreinte)
            .map_err(|erreur| format!("Marque non écrite : {erreur}"))?;

        let _ = app_tache.emit(
            "modele-progression",
            Progression {
                identifiant: id_tache.clone(),
                recus: modele.taille,
                total: modele.taille,
            },
        );
        Ok(())
    })
    .await
    .map_err(|erreur| format!("Téléchargement interrompu : {erreur}"))?;

    resultat?;
    Ok(etat_de(
        &app,
        modele,
        &crate::reglages::lire_sans_application().modele,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ Une empreinte mal recopiee rendrait ce module PIRE qu'inutile : il refuserait des
    /// fichiers parfaitement sains, et le message accuserait le reseau.
    #[test]
    fn chaque_empreinte_est_un_sha256_bien_forme() {
        for modele in CATALOGUE {
            assert_eq!(
                modele.empreinte.len(),
                64,
                "{} : une empreinte SHA-256 fait 64 caracteres",
                modele.identifiant
            );
            assert!(
                modele.empreinte.chars().all(|c| c.is_ascii_hexdigit()),
                "{} : empreinte non hexadecimale",
                modele.identifiant
            );
            assert!(
                modele.empreinte.chars().all(|c| !c.is_ascii_uppercase()),
                "{} : empreinte en majuscules, la comparaison echouerait",
                modele.identifiant
            );
        }
    }

    #[test]
    fn les_identifiants_et_les_fichiers_sont_uniques() {
        for (i, a) in CATALOGUE.iter().enumerate() {
            for b in &CATALOGUE[i + 1..] {
                assert_ne!(a.identifiant, b.identifiant);
                assert_ne!(a.fichier, b.fichier);
                assert_ne!(a.empreinte, b.empreinte);
            }
        }
    }

    /// Une taille a zero ferait passer un fichier absent pour complet.
    #[test]
    fn chaque_modele_annonce_une_taille_plausible() {
        for modele in CATALOGUE {
            assert!(
                modele.taille > 50_000_000,
                "{} : taille suspecte ({})",
                modele.identifiant,
                modele.taille
            );
        }
    }

    /// ⛔ Le coreen est propose par le produit et absent de Parakeet : c'est le cas qui doit etre
    /// refuse, et qui le serait en silence si la liste des langues etait elargie par erreur.
    #[test]
    fn parakeet_refuse_le_coreen_et_accepte_le_francais() {
        assert!(!Famille::Parakeet.connait("ko"));
        assert!(Famille::Parakeet.connait("fr"));
        assert!(Famille::Parakeet.connait("auto"));
        assert!(Famille::Whisper.connait("ko"));
    }

    /// Chaque langue que l'interface propose doit avoir un sort decide pour Parakeet, et le seul
    /// refus attendu aujourd'hui est le coreen. Une langue ajoutee a l'interface fera echouer ce
    /// test, ce qui oblige a se poser la question au lieu de la decouvrir a la dictee.
    #[test]
    fn le_sort_de_chaque_langue_de_l_interface_est_decide() {
        let interface = fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("src")
                .join("reglages.ts"),
        )
        .expect("reglages.ts lisible");
        let debut = interface
            .find("titre: 'Langue'")
            .expect("le choix de langue");
        let fin = debut
            + interface[debut..]
                .find("aide:")
                .expect("l'aide qui suit la liste");
        let bloc = &interface[debut..fin];
        let codes: Vec<&str> = bloc
            .lines()
            .filter_map(|ligne| ligne.trim().strip_prefix("['"))
            .map(|reste| reste.split('\'').next().unwrap_or(""))
            .collect();
        let refusees = |famille: Famille| -> Vec<&str> {
            codes
                .iter()
                .copied()
                .filter(|code| !famille.connait(code))
                .collect()
        };
        assert_eq!(refusees(Famille::Parakeet), vec!["ko"]);
        // SenseVoice ne connait aucune langue europeenne hors l'anglais.
        assert_eq!(refusees(Famille::SenseVoice), vec!["fr", "pt", "es", "de"]);
    }

    /// ⛔ **Un modele ajoute au catalogue sans etre mesure aurait une ligne vide dans le tableau
    /// de comparaison**, et rien ne le signalerait. Ce test oblige a relancer
    /// `scripts/mesurer-modeles.py`, qui est aussi le seul a pouvoir ecrire ces chiffres.
    #[test]
    fn chaque_modele_du_catalogue_est_mesure() {
        let fichier = fichier_mesures().expect("mesures-modeles.json illisible");
        assert!(!fichier.mesure.date.is_empty() && !fichier.mesure.machine.is_empty());
        for modele in CATALOGUE {
            let m = fichier
                .modeles
                .get(modele.identifiant)
                .unwrap_or_else(|| panic!("{} n'est pas mesure", modele.identifiant));
            assert!(
                m.fois_plus_rapide_que_la_parole > 0.0,
                "{}",
                modele.identifiant
            );
            // ⛔ Un modele qui transcrit le francais DOIT avoir son taux d'erreurs : seul celui qui
            // ne le connait pas a le droit de ne pas en avoir, sinon on cacherait une mesure.
            assert_eq!(
                m.erreurs_pour_cent.is_some(),
                modele.famille.connait("fr"),
                "{} : taux d'erreurs present si et seulement si le modele connait le francais",
                modele.identifiant
            );
            assert!(
                m.erreurs_pour_cent <= m.erreurs_pour_cent_pire_voix,
                "{}",
                modele.identifiant
            );
        }
    }

    #[test]
    fn les_urls_sont_en_https() {
        for modele in CATALOGUE {
            assert!(
                modele.url.starts_with("https://"),
                "{} : un modele ne se telecharge pas en clair",
                modele.identifiant
            );
        }
    }

    #[test]
    fn un_identifiant_inconnu_ne_rend_rien() {
        assert!(par_identifiant("inconnu").is_none());
        assert!(par_identifiant("small").is_some());
    }

    /// ⚠️ L'empreinte se calcule par blocs. Ce test verifie le resultat contre une valeur connue,
    /// pour attraper une erreur de decoupage qui donnerait un hachage stable mais faux.
    #[test]
    fn l_empreinte_d_un_contenu_connu_est_juste() {
        let dossier = std::env::temp_dir().join("oyant-essai-empreinte");
        let _ = fs::create_dir_all(&dossier);
        let fichier = dossier.join("abc.txt");
        fs::write(&fichier, b"abc").unwrap();

        // SHA-256 de « abc », valeur de reference publique.
        assert_eq!(
            empreinte(&fichier).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        let _ = fs::remove_file(&fichier);
    }
}
