//! Transcription d'un fichier audio deposé dans la fenetre.
//!
//! ⛔ **Le meme chemin que la dictee, sauf l'ecriture au curseur.** Meme resolution du moteur et du
//! modele (`dictee::resoudre`), meme detection de parole, meme mise en forme (hesitations,
//! vocabulaire, substitutions) : un fichier transcrit autrement qu'une dictee donnerait deux textes
//! differents pour la meme voix sur la meme machine, sans que rien explique pourquoi.
//!
//! ⚠️ Le texte n'est ni injecte ni ajoute a l'historique : un fichier n'est pas une dictee, et
//! l'ecrire dans la fenetre active (qui est Oyant lui-meme, puisqu'on vient d'y deposer le
//! fichier) n'aurait aucun sens. Il s'affiche, se copie, ou s'enregistre a cote du fichier.

use std::path::{Path, PathBuf};

use crate::{dictee, modeles::Famille, moteur, reglages};

/// Extensions que le moteur sait lire, selon la famille du modele.
///
/// ⚠️ whisper.cpp (Whisper et Parakeet) decode lui-meme wav, mp3, ogg et flac. `sherpa-onnx`
/// (SenseVoice) ne lit QUE le WAV : le lui donner quand meme ferait echouer le moteur avec un
/// message qui ne parle pas du format.
pub fn format_lisible(chemin: &Path, famille: Famille) -> Result<(), String> {
    let extension = chemin
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let acceptees: &[&str] = match famille {
        Famille::Whisper | Famille::Parakeet => &["wav", "mp3", "ogg", "flac"],
        Famille::SenseVoice => &["wav"],
    };
    if acceptees.contains(&extension.as_str()) {
        return Ok(());
    }
    let liste = acceptees.join(", ");
    Err(if extension.is_empty() {
        format!("Ce fichier n’a pas d’extension. Formats lus avec ce modèle : {liste}.")
    } else {
        format!("Le format .{extension} n’est pas lu avec ce modèle. Formats lus : {liste}.")
    })
}

/// Le chemin du `.txt` a ecrire a cote de l'audio, sans jamais ecraser un fichier existant.
///
/// ⛔ **Ne jamais ecraser** : un `entretien.txt` a cote de `entretien.mp3` peut etre une
/// transcription corrigee a la main, des heures de travail. On numerote plutot.
pub fn chemin_texte(audio: &Path, existe: impl Fn(&Path) -> bool) -> PathBuf {
    let premier = audio.with_extension("txt");
    if !existe(&premier) {
        return premier;
    }
    let tige = audio
        .file_stem()
        .map(|t| t.to_string_lossy().into_owned())
        .unwrap_or_else(|| "transcription".into());
    (2..)
        .map(|n| audio.with_file_name(format!("{tige} ({n}).txt")))
        .find(|candidat| !existe(candidat))
        .expect("une suite infinie finit par trouver un nom libre")
}

/// Ce que l'ecran affiche d'un fichier transcrit.
#[derive(serde::Serialize)]
pub struct FichierTranscrit {
    texte: String,
    secondes: f64,
    modele: String,
}

/// Transcrit un fichier deposé, avec les reglages de la dictee.
///
/// `async` et le travail dans un fil a part : une transcription de plusieurs minutes ne doit pas
/// figer la fenetre ni bloquer les autres commandes.
#[tauri::command]
pub async fn transcrire_fichier(chemin: String) -> Result<FichierTranscrit, String> {
    tauri::async_runtime::spawn_blocking(move || transcrire(Path::new(&chemin)))
        .await
        .map_err(|e| format!("Transcription interrompue : {e}"))?
}

fn transcrire(audio: &Path) -> Result<FichierTranscrit, String> {
    if !audio.is_file() {
        return Err("Ce n’est pas un fichier, ou il a été déplacé.".into());
    }
    let reglages = reglages::lire_sans_application();
    let outils = dictee::resoudre(&reglages.modele, &reglages.langue)
        .map_err(|manque| manque.en_francais())?;
    format_lisible(audio, outils.famille)?;

    let prompt = moteur::prompt_des_reglages(&reglages);
    let vad = crate::vad::selon(&reglages)
        .map_err(|e| format!("Détection de parole indisponible : {e}"))?;
    let resultat = moteur::transcrire(
        &outils.executable,
        &moteur::Demande {
            famille: outils.famille,
            modele: &outils.modele,
            audio,
            langue: &reglages.langue,
            fils: reglages.fils,
            temperature: reglages.temperature,
            prompt: prompt.as_deref(),
            vad: vad.as_deref(),
        },
    )?;
    let brut = resultat.texte.trim();
    if brut.is_empty() {
        return Err("Aucune parole reconnue dans ce fichier.".into());
    }
    Ok(FichierTranscrit {
        texte: dictee::mettre_en_forme(brut, &reglages),
        secondes: resultat.secondes,
        // Le nom affiche dans le catalogue, pas l'identifiant interne (« Medium » et pas
        // « medium »), avec l'identifiant en repli s'il venait a manquer.
        modele: crate::modeles::par_identifiant(&reglages.modele)
            .map(|m| m.nom.to_string())
            .unwrap_or(reglages.modele),
    })
}

/// Enregistre le texte dans un `.txt` a cote du fichier audio, et rend le chemin ecrit.
#[tauri::command]
pub fn enregistrer_texte_a_cote(audio: String, texte: String) -> Result<String, String> {
    let cible = chemin_texte(Path::new(&audio), |p| p.exists());
    std::fs::write(&cible, texte).map_err(|e| format!("Écriture impossible : {e}"))?;
    Ok(cible.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_et_parakeet_lisent_les_quatre_formats_du_moteur() {
        for nom in ["a.wav", "b.MP3", "c.ogg", "d.flac"] {
            assert!(
                format_lisible(Path::new(nom), Famille::Whisper).is_ok(),
                "{nom}"
            );
            assert!(
                format_lisible(Path::new(nom), Famille::Parakeet).is_ok(),
                "{nom}"
            );
        }
    }

    #[test]
    fn sensevoice_ne_lit_que_le_wav() {
        assert!(format_lisible(Path::new("a.wav"), Famille::SenseVoice).is_ok());
        let refus = format_lisible(Path::new("a.mp3"), Famille::SenseVoice).unwrap_err();
        assert!(refus.contains(".mp3") && refus.contains("wav"), "{refus}");
    }

    #[test]
    fn un_format_inconnu_est_refuse_en_le_nommant() {
        let refus = format_lisible(Path::new("note.m4a"), Famille::Whisper).unwrap_err();
        assert!(refus.contains(".m4a"), "{refus}");
        assert!(format_lisible(Path::new("sans-extension"), Famille::Whisper).is_err());
    }

    #[test]
    fn le_texte_va_a_cote_de_l_audio() {
        let audio = Path::new("/dossier/entretien.mp3");
        assert_eq!(
            chemin_texte(audio, |_| false),
            Path::new("/dossier/entretien.txt")
        );
    }

    #[test]
    fn un_texte_existant_n_est_jamais_ecrase() {
        let audio = Path::new("/dossier/entretien.mp3");
        let pris = [
            PathBuf::from("/dossier/entretien.txt"),
            PathBuf::from("/dossier/entretien (2).txt"),
        ];
        assert_eq!(
            chemin_texte(audio, |p| pris.iter().any(|q| q == p)),
            Path::new("/dossier/entretien (3).txt")
        );
    }
}
