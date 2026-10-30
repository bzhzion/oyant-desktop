//! Le modele de detection de parole (Silero VAD), donne a `whisper-cli --vad`.
//!
//! ⛔ **Pourquoi il existe, mesure le 2026-10-29 sur `office`.** Sans lui, Whisper tape dans
//! l'application de l'utilisateur des phrases qu'il n'a jamais prononcees : sur six secondes de
//! souffle de ventilation, Small a rendu « (Sous-titres réalises par la communaute d'Amara.org) ».
//! Ce souffle passait le seuil de silence (niveau 0,0123 pour un seuil a 0,01), donc rien ne
//! l'arretait avant le moteur. Avec le VAD, la sortie est vide, en 0,7 s au lieu de 4.
//!
//! ⛔ **Et Medium perdait du texte apres une pause** : sur trente secondes commencant par un titre
//! suivi d'un silence, il n'a rendu que la premiere phrase (29 caracteres sur 373). Le VAD ne lui
//! donne que les passages parles, et le texte revient en entier.
//!
//! ⚠️ **Integre a l'executable, pas telecharge.** 885 Ko ne justifient ni un telechargement, ni
//! une dependance au reseau, ni une difference entre Windows (moteur embarque) et Linux (moteur
//! telecharge). Il est ecrit une fois dans le repertoire des modeles, parce que `whisper-cli` lit
//! un chemin de fichier et pas des octets.

use std::fs;
use std::path::PathBuf;

/// Le modele, au format ggml de whisper.cpp (`ggml-org/whisper-vad` sur HuggingFace).
const OCTETS: &[u8] = include_bytes!("../vad/ggml-silero-v6.2.0.bin");

/// Nom du fichier ecrit sur le disque. La version y figure : un modele plus recent prendra un
/// autre nom au lieu d'ecraser celui qu'un moteur en cours d'execution est peut-etre en train de
/// lire.
const FICHIER: &str = "ggml-silero-v6.2.0.bin";

/// Empreinte SHA-256, **lue sur l'API HuggingFace** (identifiant LFS) le 2026-10-29.
#[cfg(test)]
const EMPREINTE: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";

/// Le chemin du modele sur le disque, ecrit au premier appel.
///
/// ⚠️ **Compare sur la TAILLE, pas sur l'empreinte**, a chaque dictee : relire 885 Ko pour les
/// hacher a chaque appui serait du temps pris sur la dictee pour un risque que l'ecriture
/// atomique couvre deja. L'empreinte des octets integres, elle, est verifiee par un test.
///
/// ⛔ **Ecriture dans un fichier temporaire puis renommage** : deux dictees rapprochees, ou la
/// dictee et le telephone, pourraient sinon ecrire en meme temps et laisser un modele tronque,
/// que la comparaison de taille rattraperait au prochain appel, mais apres un echec.
pub fn chemin() -> Result<PathBuf, String> {
    let dossier = crate::chemins::modeles()?;
    let chemin = dossier.join(FICHIER);
    if fs::metadata(&chemin).map(|m| m.len()).ok() == Some(OCTETS.len() as u64) {
        return Ok(chemin);
    }

    fs::create_dir_all(&dossier)
        .map_err(|erreur| format!("Répertoire des modèles inaccessible : {erreur}"))?;
    let temporaire = dossier.join(format!("{FICHIER}.{}.partiel", std::process::id()));
    fs::write(&temporaire, OCTETS)
        .map_err(|erreur| format!("Écriture du modèle de détection impossible : {erreur}"))?;
    fs::rename(&temporaire, &chemin).map_err(|erreur| {
        let _ = fs::remove_file(&temporaire);
        format!("Écriture du modèle de détection impossible : {erreur}")
    })?;
    Ok(chemin)
}

/// Le modele a passer au moteur selon les reglages, `None` si la detection est coupee.
///
/// ⛔ **Une seule fonction pour la dictee, la ligne de commande et le telephone.** Trois copies de
/// cette decision finiraient par diverger, et c'est toujours celle qu'on ne teste pas a la main
/// (le telephone) qui garderait l'ancien comportement.
pub fn selon(reglages: &crate::reglages::Reglages) -> Result<Option<PathBuf>, String> {
    if reglages.detection_parole {
        chemin().map(Some)
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// ⛔ **Le fichier integre est-il le bon ?** Un `git` mal configure, une conversion de fin de
    /// ligne ou un remplacement par megarde rendraient un fichier de la bonne taille et inutilisable,
    /// et `whisper-cli` echouerait a chaque dictee sur un message de format.
    #[test]
    fn le_modele_integre_a_l_empreinte_publiee() {
        let resume = Sha256::digest(OCTETS);
        let hexadecimal: String = resume.iter().map(|octet| format!("{octet:02x}")).collect();
        assert_eq!(hexadecimal, EMPREINTE);
    }
}
