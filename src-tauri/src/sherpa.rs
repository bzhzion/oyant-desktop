//! Le programme qui fait tourner SenseVoice : `sherpa-onnx-offline`, du projet sherpa-onnx.
//!
//! ⚠️ **Pourquoi un second programme a cote de whisper.cpp.** SenseVoice n'est pas un modele que
//! whisper.cpp sait lire, il passe par ONNX Runtime. Mesure du 2026-10-31 sur un extrait en
//! coreen de 4,6 s : 1,8 s chargement compris (0,11 s de calcul), contre 4,3 s pour Whisper Small,
//! meme texte. C'est le moteur rapide pour le coreen, le chinois et le japonais, langues que
//! Parakeet ne connait pas.
//!
//! Le programme se telecharge au premier choix de SenseVoice, depuis la publication officielle,
//! et son empreinte est verifiee avant d'etre deballe, comme pour les moteurs de whisper.cpp.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use crate::moteur::{FormatArchive, deballer, empreinte_de, format_archive};

/// Une publication de sherpa-onnx pour une plateforme.
///
/// ⚠️ Taille et empreinte RELEVEES sur l'API GitHub (champ `digest`) le 2026-10-31.
///
/// Jamais construit sur une plateforme sans publication retenue (macOS, ARM) : le lint le
/// signalerait comme du code mort, alors que c'est l'absence voulue de `PROGRAMME`.
#[cfg_attr(
    not(any(
        all(windows, target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )),
    allow(dead_code)
)]
struct Programme {
    url: &'static str,
    taille: u64,
    empreinte: &'static str,
    prefixe: &'static str,
    executable: &'static str,
}

#[cfg(all(windows, target_arch = "x86_64"))]
const PROGRAMME: Option<Programme> = Some(Programme {
    // `no-tts` : sans la synthese vocale, dont on n'a pas l'usage et qui pese le double.
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-win-x64-shared-MD-Release-no-tts.tar.bz2",
    taille: 20_962_760,
    empreinte: "11fd6a50b97eed38b54fb16d8219f49b16aa7f6a8e43a042316469ef47652e82",
    prefixe: "sherpa-onnx-v2.0.1-win-x64-shared-MD-Release-no-tts/",
    executable: "bin/sherpa-onnx-offline.exe",
});

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const PROGRAMME: Option<Programme> = Some(Programme {
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.1/sherpa-onnx-v2.0.1-linux-x64-shared-no-tts.tar.bz2",
    taille: 25_289_904,
    empreinte: "b3b095f619059d54914027ff38f8a0a132ea3bbc5df32a212a9743a8bd1467b2",
    prefixe: "sherpa-onnx-v2.0.1-linux-x64-shared-no-tts/",
    executable: "bin/sherpa-onnx-offline",
});

/// Plateforme sans publication retenue : SenseVoice y est refuse avec un message, jamais devine.
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
const PROGRAMME: Option<Programme> = None;

/// Le vocabulaire du modele, integre a l'executable (316 Ko).
///
/// ⚠️ Integre plutot que telecharge : le catalogue des modeles ne sait telecharger qu'un fichier
/// par modele, et ce fichier-ci est trop petit pour justifier un second circuit. Licence du modele
/// (FunASR, attribution requise) dans `NOTICE.md`.
const TOKENS: &[u8] = include_bytes!("../sensevoice/tokens.txt");
pub const FICHIER_TOKENS: &str = "sensevoice-tokens.txt";

/// Empreinte du vocabulaire, calculee le 2026-10-31 sur le fichier publie.
#[cfg(test)]
const EMPREINTE_TOKENS: &str = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc";

/// SenseVoice peut-il tourner sur cette plateforme ?
pub fn disponible() -> bool {
    PROGRAMME.is_some()
}

fn dossier() -> Result<PathBuf, String> {
    Ok(crate::chemins::moteurs()?.join("sherpa-onnx"))
}

fn marque() -> Result<PathBuf, String> {
    Ok(dossier()?.join("installe.empreinte"))
}

/// Le programme, s'il est installe et que c'est bien la version attendue.
pub fn executable() -> Option<PathBuf> {
    let programme = PROGRAMME.as_ref()?;
    let installe = fs::read_to_string(marque().ok()?).ok()?;
    if installe.trim() != programme.empreinte {
        return None;
    }
    let chemin = dossier().ok()?.join(programme.executable);
    chemin.is_file().then_some(chemin)
}

/// Telecharge, verifie et deballe le programme. Bloquant : a appeler hors du fil de l'interface.
pub fn installer() -> Result<(), String> {
    let programme = PROGRAMME
        .as_ref()
        .ok_or("SenseVoice n'est pas disponible sur ce système.")?;
    let cible = dossier()?;
    let parent = cible.parent().ok_or("Répertoire parent introuvable")?;
    fs::create_dir_all(parent).map_err(|e| format!("Répertoire non créé : {e}"))?;
    let archive = parent.join("sherpa-onnx.partiel");

    let reponse = ureq::get(programme.url)
        .call()
        .map_err(|e| format!("Téléchargement impossible : {e}"))?;
    let mut octets = Vec::with_capacity(programme.taille as usize);
    reponse
        .into_reader()
        .take(programme.taille + 1)
        .read_to_end(&mut octets)
        .map_err(|e| format!("Transfert interrompu : {e}"))?;
    fs::write(&archive, &octets).map_err(|e| format!("Écriture impossible : {e}"))?;

    // ⛔ L'empreinte AVANT de deballer : une archive abimee ou substituee ne doit rien poser.
    if empreinte_de(&archive)? != programme.empreinte {
        let _ = fs::remove_file(&archive);
        return Err(
            "Le programme de SenseVoice est arrivé abîmé : son empreinte ne correspond pas. \
             Rien n'a été installé."
                .to_string(),
        );
    }

    if cible.exists() {
        fs::remove_dir_all(&cible).map_err(|e| format!("Ancienne version non retirée : {e}"))?;
    }
    fs::create_dir_all(&cible).map_err(|e| format!("Répertoire non créé : {e}"))?;
    let format = format_archive(programme.url);
    debug_assert_eq!(format, FormatArchive::TarBz2);
    deballer(&archive, format, &cible, programme.prefixe)?;
    let _ = fs::remove_file(&archive);

    // La marque EN DERNIER : une extraction interrompue ne se declare pas installee.
    fs::write(marque()?, programme.empreinte).map_err(|e| format!("Marque non écrite : {e}"))?;
    Ok(())
}

/// Le chemin du vocabulaire, ecrit a cote des modeles s'il manque.
///
/// ⚠️ Ecriture dans un temporaire puis renommage, comme le modele de detection de parole : deux
/// dictees rapprochees ne doivent pas laisser un fichier tronque.
pub fn tokens() -> Result<PathBuf, String> {
    let dossier = crate::chemins::modeles()?;
    let chemin = dossier.join(FICHIER_TOKENS);
    if fs::metadata(&chemin).map(|m| m.len()).ok() == Some(TOKENS.len() as u64) {
        return Ok(chemin);
    }
    fs::create_dir_all(&dossier)
        .map_err(|e| format!("Répertoire des modèles inaccessible : {e}"))?;
    let temporaire = dossier.join(format!("{FICHIER_TOKENS}.{}.partiel", std::process::id()));
    fs::write(&temporaire, TOKENS).map_err(|e| format!("Vocabulaire non écrit : {e}"))?;
    fs::rename(&temporaire, &chemin).map_err(|e| {
        let _ = fs::remove_file(&temporaire);
        format!("Vocabulaire non écrit : {e}")
    })?;
    Ok(chemin)
}

/// Le texte d'une sortie de `sherpa-onnx-offline`, qui ecrit une ligne JSON par fichier.
///
/// Fonction PURE. Une sortie sans ligne JSON lisible rend une chaine vide plutot qu'une erreur :
/// c'est le cas d'un enregistrement sans parole, que la dictee traite deja comme « rien a ecrire ».
pub fn texte_de_la_sortie(sortie: &str) -> String {
    sortie
        .lines()
        .filter(|ligne| ligne.trim_start().starts_with('{'))
        .filter_map(|ligne| serde_json::from_str::<serde_json::Value>(ligne).ok())
        .filter_map(|valeur| {
            valeur
                .get("text")?
                .as_str()
                .map(str::trim)
                .map(String::from)
        })
        .filter(|texte| !texte.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// ⛔ Une conversion des fins de ligne par git (`\n` en `\r\n` sous Windows) garderait un
    /// fichier lisible et ferait echouer SenseVoice a chaque dictee. D'ou `-text` dans
    /// `sensevoice/.gitattributes`, et ce test.
    #[test]
    fn le_vocabulaire_integre_est_le_fichier_publie() {
        let resume = Sha256::digest(TOKENS);
        let hexadecimal: String = resume.iter().map(|o| format!("{o:02x}")).collect();
        assert_eq!(hexadecimal, EMPREINTE_TOKENS);
    }

    #[test]
    fn le_texte_est_lu_dans_la_ligne_json() {
        let sortie = r#"{"lang": "<|ko|>", "emotion": "<|NEUTRAL|>", "text": "조금만 생각을 하면서 살면 훨씬 편할 거야.", "tokens": []}"#;
        assert_eq!(
            texte_de_la_sortie(sortie),
            "조금만 생각을 하면서 살면 훨씬 편할 거야."
        );
    }

    #[test]
    fn une_sortie_sans_json_rend_un_texte_vide() {
        assert_eq!(texte_de_la_sortie("Done!\nElapsed seconds: 0.1\n"), "");
        assert_eq!(texte_de_la_sortie(r#"{"text": "  "}"#), "");
    }

    /// De bout en bout, sur le vrai reseau : installe le programme comme le fait Oyant, puis
    /// transcrit un extrait reel. `OYANT_SENSEVOICE_MODELE` donne le modele, `OYANT_SENSEVOICE_AUDIO`
    /// un fichier en coreen, `OYANT_SENSEVOICE_ATTENDU` un fragment du texte attendu.
    #[test]
    #[ignore = "telecharge 21 Mo ; variables OYANT_SENSEVOICE_* requises"]
    fn installation_et_transcription_reelles() {
        installer().expect("installation");
        let executable = executable().expect("programme installe et reconnu");
        let modele = std::path::PathBuf::from(std::env::var("OYANT_SENSEVOICE_MODELE").unwrap());
        let audio = std::path::PathBuf::from(std::env::var("OYANT_SENSEVOICE_AUDIO").unwrap());
        let attendu = std::env::var("OYANT_SENSEVOICE_ATTENDU").unwrap();
        let dossier_modele = modele.parent().unwrap();
        fs::write(dossier_modele.join(FICHIER_TOKENS), TOKENS).unwrap();
        let resultat = crate::moteur::transcrire(
            &executable,
            &crate::moteur::Demande {
                famille: crate::modeles::Famille::SenseVoice,
                modele: &modele,
                audio: &audio,
                langue: "ko",
                fils: 4,
                temperature: 0.0,
                prompt: None,
                vad: None,
            },
        )
        .expect("transcription");
        eprintln!("texte : {}", resultat.texte);
        assert!(resultat.texte.contains(&attendu), "{}", resultat.texte);
    }

    #[test]
    fn la_publication_est_une_archive_tar_bz2() {
        if let Some(programme) = PROGRAMME.as_ref() {
            assert_eq!(format_archive(programme.url), FormatArchive::TarBz2);
            assert_eq!(programme.empreinte.len(), 64);
        }
    }
}
